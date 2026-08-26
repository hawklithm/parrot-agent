//! HTTP parity integration tests for Paperclip's Company Search (#4C.3).
//!
//! `GET /companies/:company_id/search` returns the `CompanySearchResponse`
//! shape (issue scope: title/identifier/description full-text + token match),
//! with `q`/`scope`/`limit`/`offset`/`sort`, tenant isolation, and cross-company 403.
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test company_search_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::company_routes;
use parrot_server::build_app_state;
use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};

async fn send(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
) -> (StatusCode, Vec<u8>) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .expect("build request");
    let mut req = req;
    req.extensions_mut().insert(actor.clone());
    let resp = app.clone().oneshot(req).await.expect("dispatch request");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");
    (status, bytes.to_vec())
}

fn parse(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).expect("response body must be JSON")
}

fn board_actor(user_id: Uuid, company_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::board(user_id, company_id)
}

fn session_board_actor(user_id: Uuid, company_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::board_with_source(
        user_id,
        company_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            company_id,
            PrincipalType::User,
            user_id,
            MembershipRole::Operator,
        )],
        false,
    )
}

struct Fixture {
    pool: PgPool,
    company_a: Uuid,
    company_b: Uuid,
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    let company_b = Uuid::new_v4();
    let prefix_a = format!("CS{}", &company_a.simple().to_string()[..8]);
    let prefix_b = format!("CS{}", &company_b.simple().to_string()[..8]);

    for (id, name) in [
        (company_a, "Search Parity Co A"),
        (company_b, "Search Parity Co B"),
    ] {
        sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(name)
            .bind(if id == company_a {
                &prefix_a
            } else {
                &prefix_b
            })
        .execute(pool)
        .await
        .expect("insert company");
    }

    // Company A issues: one title match, one identifier match, one description match.
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, identifier, status, priority, description) \
         VALUES ($1, $2, $3, $4, 'todo', 'medium', $5)",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind("Login button broken on homepage")
    .bind(format!("{}-1", prefix_a))
    .bind("unrelated body text")
    .execute(pool)
    .await
    .expect("insert title issue");

    sqlx::query(
        "INSERT INTO issues (id, company_id, title, identifier, status, priority, description) \
         VALUES ($1, $2, $3, $4, 'in_progress', 'high', $5)",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind("Random task")
    .bind(format!("{}-loginflow", prefix_a))
    .bind("unrelated body text")
    .execute(pool)
    .await
    .expect("insert identifier issue");

    sqlx::query(
        "INSERT INTO issues (id, company_id, title, identifier, status, priority, description) \
         VALUES ($1, $2, $3, $4, 'backlog', 'low', $5)",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind("Another task")
    .bind(format!("{}-3", prefix_a))
    .bind("the migration broke the login endpoint last night")
    .execute(pool)
    .await
    .expect("insert description issue");

    // Company B issue with the same keyword must NOT appear in company A search.
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, identifier, status, priority, description) \
         VALUES ($1, $2, $3, $4, 'todo', 'medium', $5)",
    )
    .bind(Uuid::new_v4())
    .bind(company_b)
    .bind("login outage on prod")
    .bind(format!("{}-1", prefix_b))
    .bind("leak")
    .execute(pool)
    .await
    .expect("insert company B issue");

    Fixture {
        pool: pool.clone(),
        company_a,
        company_b,
    }
}

async fn cleanup_fixture(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
        .bind(f.company_b)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(f.company_b)
        .execute(&f.pool)
        .await;
}

async fn connect_and_migrate() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile".to_string()
    });
    let pool = PgPool::connect(&database_url)
        .await
        .expect("connect database for company search HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

/// #4C.3 Company Search acceptance — issue scope, tenant isolation, pagination, cross-company 403.
#[tokio::test]
async fn company_search_matches_paperclip() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone())
        .await
        .expect("build_app_state");
    let app = company_routes().with_state(state);
    let board = board_actor(Uuid::new_v4(), f.company_a);

    // Title phrase match + tenant isolation (company B "login" issue excluded).
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search?q=login", f.company_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "search → 200");
    let resp = parse(&body);
    assert_eq!(resp["query"], json!("login"));
    assert_eq!(resp["scope"], json!("all"));
    assert_eq!(resp["sort"], json!("relevance"));
    let results = resp["results"].as_array().expect("results array");
    assert_eq!(
        results.len(),
        3,
        "title + identifier + description match, no cross-company leak"
    );
    for r in results {
        assert_eq!(r["type"], json!("issue"));
        assert!(r["issue"].is_object(), "issue summary present");
        assert!(r["matchedFields"].as_array().unwrap().len() >= 1);
    }
    // snippet/highlight：标题命中应有 field=title 的摘录且高亮覆盖 "login"。
    let title_snip = results
        .iter()
        .flat_map(|r| r["snippets"].as_array().cloned().unwrap_or_default())
        .find(|s| s["field"].as_str() == Some("title"));
    assert!(title_snip.is_some(), "title snippet present");
    let ts = title_snip.unwrap();
    assert!(
        ts["text"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("login"),
        "title snippet contains query"
    );
    assert!(
        !ts["highlights"].as_array().unwrap().is_empty(),
        "title snippet highlighted"
    );
    assert_eq!(resp["countsByType"]["issue"], json!(3));
    assert_eq!(resp["hasMore"], json!(false));

    // filterOptionCounts：facet 计数覆盖全候选命中集（3 个 issue 的不同 status/priority）。
    let foc = &resp["filterOptionCounts"];
    assert_eq!(foc["status"]["todo"], json!(1), "facet status.todo");
    assert_eq!(
        foc["status"]["in_progress"],
        json!(1),
        "facet status.in_progress"
    );
    assert_eq!(foc["status"]["backlog"], json!(1), "facet status.backlog");
    assert_eq!(foc["priority"]["medium"], json!(1), "facet priority.medium");
    assert_eq!(foc["priority"]["high"], json!(1), "facet priority.high");
    assert_eq!(foc["priority"]["low"], json!(1), "facet priority.low");
    assert!(
        foc["assigneeAgentId"].as_object().unwrap().is_empty(),
        "no assigneeAgent facets"
    );
    assert!(
        foc["assigneeUserId"].as_object().unwrap().is_empty(),
        "no assigneeUser facets"
    );
    assert!(
        foc["projectId"].as_object().unwrap().is_empty(),
        "no project facets"
    );
    assert!(
        foc["labelId"].as_object().unwrap().is_empty(),
        "no label facets"
    );
    for k in ["24h", "7d", "30d", "90d"] {
        assert_eq!(
            foc["updatedWithin"][k],
            json!(3),
            "facet updatedWithin.{}",
            k
        );
    }

    // Identifier match via scoped token (loginflow).
    let (_, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search?q=loginflow", f.company_a),
    )
    .await;
    let resp = parse(&body);
    let results = resp["results"].as_array().expect("results array");
    assert_eq!(results.len(), 1, "identifier token match");
    assert_eq!(results[0]["matchedFields"][0], json!("identifier"));

    // Scope=issues restricts to issue text hits (same as all for this dataset).
    let (_, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search?q=login&scope=issues", f.company_a),
    )
    .await;
    let resp = parse(&body);
    assert_eq!(resp["scope"], json!("issues"));
    assert_eq!(resp["results"].as_array().unwrap().len(), 3);

    // Scope=agents returns an empty result for this fixture, not an error.
    let (_, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search?q=login&scope=agents", f.company_a),
    )
    .await;
    let resp = parse(&body);
    assert_eq!(resp["results"].as_array().unwrap().len(), 0);

    // Empty query → empty results (Paperclip contract).
    let (_, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search", f.company_a),
    )
    .await;
    let resp = parse(&body);
    assert_eq!(resp["results"].as_array().unwrap().len(), 0);

    // Pagination: limit + offset.
    let (_, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search?q=login&limit=1&offset=1", f.company_a),
    )
    .await;
    let resp = parse(&body);
    let page = resp["results"].as_array().expect("results array");
    assert_eq!(page.len(), 1, "limit=1");
    assert_eq!(resp["limit"], json!(1));
    assert_eq!(resp["offset"], json!(1));
    assert_eq!(
        resp["hasMore"],
        json!(true),
        "more results beyond offset+limit"
    );

    // Sort=updated returns 200 and preserves the response shape.
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search?q=login&sort=updated", f.company_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(parse(&body)["sort"], json!("updated"));

    // Cross-company board cannot read company A search.
    let outsider = session_board_actor(Uuid::new_v4(), f.company_b);
    let (status, _) = send(
        &app,
        &outsider,
        "GET",
        &format!("/companies/{}/search?q=login", f.company_a),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-company search → 403");

    cleanup_fixture(&f).await;
}

/// #4C.3 Company Search acceptance - active issue filters, zero-result recovery,
/// artifact projections, and offset after cross-type ranking.
#[tokio::test]
async fn company_search_filters_zero_results_artifacts_and_global_offset() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let project_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects (id, company_id, name, description, status) \
         VALUES ($1, $2, $3, $4, 'in_progress')",
    )
    .bind(project_id)
    .bind(f.company_a)
    .bind("Research Project")
    .bind("research workspace")
    .execute(&pool)
    .await
    .expect("insert search project");

    let label_id = Uuid::new_v4();
    sqlx::query("INSERT INTO labels (id, company_id, name) VALUES ($1, $2, $3)")
        .bind(label_id)
        .bind(f.company_a)
        .bind("Search Label")
        .execute(&pool)
        .await
        .expect("insert search label");

    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, role, status) \
         VALUES ($1, $2, $3, 'researcher', 'idle')",
    )
    .bind(agent_id)
    .bind(f.company_a)
    .bind("Sales Researcher")
    .execute(&pool)
    .await
    .expect("insert search agent");

    let matching_issue: Uuid = sqlx::query_scalar(
        "SELECT id FROM issues WHERE company_id = $1 AND status = 'backlog' AND priority = 'low' \
         ORDER BY identifier LIMIT 1",
    )
    .bind(f.company_a)
    .fetch_one(&pool)
    .await
    .expect("find filter issue");
    sqlx::query("UPDATE issues SET project_id = $1, assignee_agent_id = $2 WHERE id = $3")
        .bind(project_id)
        .bind(agent_id)
        .bind(matching_issue)
        .execute(&pool)
        .await
        .expect("attach issue filters");
    sqlx::query("INSERT INTO issue_labels (company_id, issue_id, label_id) VALUES ($1, $2, $3)")
        .bind(f.company_a)
        .bind(matching_issue)
        .bind(label_id)
        .execute(&pool)
        .await
        .expect("attach search label");

    let document_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO documents (id, company_id, title, content, content_type, created_by_agent_id) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(document_id)
    .bind(f.company_a)
    .bind("Generated Search Artifact")
    .bind("searchable artifact body")
    .bind("text/markdown")
    .bind(agent_id)
    .execute(&pool)
    .await
    .expect("insert artifact document");
    sqlx::query(
        "INSERT INTO issue_documents (company_id, issue_id, document_id, key) \
         VALUES ($1, $2, $3, 'generated-artifact')",
    )
    .bind(f.company_a)
    .bind(matching_issue)
    .bind(document_id)
    .execute(&pool)
    .await
    .expect("link artifact document");

    let state = build_app_state(pool.clone())
        .await
        .expect("build_app_state");
    let app = company_routes().with_state(state);
    let board = board_actor(Uuid::new_v4(), f.company_a);

    // Filter-only queries return issue rows while simple entities remain suppressed.
    let filter_uri = format!(
        "/companies/{}/search?scope=issues&status=backlog&priority=low&assigneeAgentId={}&projectId={}&labelId={}&updatedWithin=24h",
        f.company_a, agent_id, project_id, label_id
    );
    let (status, body) = send(&app, &board, "GET", &filter_uri).await;
    assert_eq!(status, StatusCode::OK);
    let resp = parse(&body);
    assert_eq!(resp["query"], json!(""));
    assert_eq!(resp["normalizedQuery"], json!(""));
    let results = resp["results"].as_array().expect("filter results array");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["id"], json!(matching_issue));
    assert_eq!(resp["countsByType"]["issue"], json!(1));
    assert_eq!(resp["countsByType"]["agent"], json!(0));
    assert_eq!(resp["countsByType"]["project"], json!(0));
    assert_eq!(resp["countsByType"]["artifact"], json!(0));
    assert_eq!(resp["filterOptionCounts"]["status"]["backlog"], json!(1));
    assert_eq!(resp["filterOptionCounts"]["priority"]["low"], json!(1));
    assert_eq!(
        resp["filterOptionCounts"]["assigneeAgentId"][agent_id.to_string()],
        json!(1)
    );
    assert_eq!(
        resp["filterOptionCounts"]["projectId"][project_id.to_string()],
        json!(1)
    );
    assert_eq!(
        resp["filterOptionCounts"]["labelId"][label_id.to_string()],
        json!(1)
    );
    assert_eq!(resp["filterOptionCounts"]["updatedWithin"]["24h"], json!(1));

    // Artifact scope projects the linked generated document as a first-class result.
    let artifact_uri = format!(
        "/companies/{}/search?q=searchable%20artifact&scope=artifacts",
        f.company_a
    );
    let (status, body) = send(&app, &board, "GET", &artifact_uri).await;
    assert_eq!(status, StatusCode::OK);
    let resp = parse(&body);
    let results = resp["results"].as_array().expect("artifact results array");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["type"], json!("artifact"));
    assert!(results[0]["id"]
        .as_str()
        .unwrap_or_default()
        .starts_with("document:"));
    assert_eq!(results[0]["artifact"]["source"], json!("document"));
    assert_eq!(results[0]["artifact"]["issueId"], json!(matching_issue));
    assert!(results[0]["href"]
        .as_str()
        .unwrap_or_default()
        .contains("#document-generated-artifact"));

    // A conflicting status/priority pair yields zero-result recovery suggestions.
    let (_, body) = send(
        &app,
        &board,
        "GET",
        &format!(
            "/companies/{}/search?q=login&status=done&priority=high",
            f.company_a
        ),
    )
    .await;
    let resp = parse(&body);
    assert_eq!(resp["results"].as_array().unwrap().len(), 0);
    assert!(resp["zeroResults"]["unfilteredTotal"].as_i64().unwrap_or(0) >= 3);
    let suggestions = resp["zeroResults"]["loosenSuggestions"]
        .as_array()
        .expect("zero-result suggestions");
    let status_suggestion = suggestions
        .iter()
        .find(|suggestion| suggestion["filter"] == json!("status"))
        .expect("status loosen suggestion");
    assert_eq!(status_suggestion["values"], json!(["done"]));
    assert!(status_suggestion["resultCount"].as_i64().unwrap_or(0) >= 1);

    // Offset is applied after the project/agent results share one relevance order.
    let (_, body) = send(
        &app,
        &board,
        "GET",
        &format!(
            "/companies/{}/search?q=research&limit=1&offset=1",
            f.company_a
        ),
    )
    .await;
    let resp = parse(&body);
    let results = resp["results"].as_array().expect("merged results array");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["type"], json!("agent"));
    assert_eq!(resp["hasMore"], json!(false));

    // Cleanup in dependency order; company labels do not cascade in the legacy schema.
    let _ = sqlx::query("DELETE FROM issue_labels WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM issue_documents WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM documents WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM labels WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM projects WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM agents WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(f.company_a)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
        .bind(f.company_b)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(f.company_b)
        .execute(&pool)
        .await;
}

/// #4C.3 Company Search acceptance - comments + documents scope in the main
/// search (§4C.3 scope expansion). An issue surfaces when a comment or linked
/// document matches the query, with `matchedFields` + `countsByType` reflecting it.
#[tokio::test]
async fn company_search_comments_documents_matches_paperclip() {
    let pool = connect_and_migrate().await;
    let user = Uuid::new_v4();
    let company_a = Uuid::new_v4();
    let prefix_a = format!("CS{}", &company_a.simple().to_string()[..8]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_a)
        .bind("Search Scope Co A")
        .bind(&prefix_a)
        .execute(&pool)
        .await
        .expect("insert company");

    let artifact_agent_id = Uuid::new_v4();
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, $3)")
        .bind(artifact_agent_id)
        .bind(company_a)
        .bind("Artifact Writer")
        .execute(&pool)
        .await
        .expect("insert artifact agent");

    // Issue with NO title/description/identifier match for "parrotsearch".
    let issue_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, identifier, status, priority, description) \
         VALUES ($1, $2, $3, $4, 'todo', 'medium', $5)",
    )
    .bind(issue_id)
    .bind(company_a)
    .bind("Plain task with no keyword")
    .bind(format!("{}-1", prefix_a))
    .bind("unrelated body with ![logo](https://cdn.example.com/logo.png) inside")
    .execute(&pool)
    .await
    .expect("insert issue");
    // Comment containing the keyword.
    sqlx::query(
        "INSERT INTO issue_comments (id, company_id, issue_id, body, actor_type) \
         VALUES ($1, $2, $3, $4, 'user')",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind(issue_id)
    .bind("customer mentioned parrotsearch in passing")
    .execute(&pool)
    .await
    .expect("insert comment");
    // Linked document containing the keyword (in content).
    let doc_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO documents (id, company_id, title, content, content_type, created_by_agent_id) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(doc_id)
    .bind(company_a)
    .bind("Runbook")
    .bind("restart the parrotsearch service after deploy")
    .bind("text/markdown")
    .bind(artifact_agent_id)
    .execute(&pool)
    .await
    .expect("insert document");
    sqlx::query(
        "INSERT INTO issue_documents (id, company_id, issue_id, document_id, key) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind(issue_id)
    .bind(doc_id)
    .bind("runbook")
    .execute(&pool)
    .await
    .expect("link document");

    let state = build_app_state(pool.clone())
        .await
        .expect("build_app_state");
    let app = company_routes().with_state(state);
    let board = board_actor(user, company_a);

    // scope=all: the issue surfaces via comment + document, and the linked
    // document is also an artifact candidate in the merged result set.
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search?q=parrotsearch", company_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let resp = parse(&body);
    let results = resp["results"].as_array().expect("results array");
    assert_eq!(
        results.len(),
        2,
        "issue plus linked document artifact surface"
    );
    let issue_result = results
        .iter()
        .find(|result| result["type"] == json!("issue"))
        .expect("issue result present");
    let mf: Vec<String> = issue_result["matchedFields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(
        mf.contains(&"comment".to_string()),
        "matchedFields includes comment: {:?}",
        mf
    );
    assert!(
        mf.contains(&"document".to_string()),
        "matchedFields includes document: {:?}",
        mf
    );

    // snippet/highlight：评论或文档命中的摘录非空，且高亮区间覆盖关键词。
    let snips = issue_result["snippets"].as_array().expect("snippets array");
    assert!(
        !snips.is_empty(),
        "snippets non-empty for comment/document match"
    );
    assert!(issue_result["snippet"].is_string(), "snippet scalar set");
    let has_kw = snips.iter().any(|s| {
        let text = s["text"].as_str().unwrap_or("");
        let ok_field = ["comment", "document"].contains(&s["field"].as_str().unwrap_or(""));
        let has_hl = s["highlights"]
            .as_array()
            .map(|h| !h.is_empty())
            .unwrap_or(false);
        ok_field && text.to_lowercase().contains("parrotsearch") && has_hl
    });
    assert!(
        has_kw,
        "a snippet surfaces the parrotsearch hit with a highlight: {:?}",
        snips
    );

    // previewImageUrl：描述中的 markdown 首图（对齐 Paperclip extractFirstImageUrl）。
    assert_eq!(
        issue_result["previewImageUrl"],
        json!("https://cdn.example.com/logo.png"),
        "description image URL surfaces as previewImageUrl"
    );
    assert_eq!(resp["countsByType"]["issue"], json!(1));
    assert_eq!(resp["countsByType"]["artifact"], json!(1));
    assert_eq!(resp["countsByType"]["comment"], json!(1));
    assert_eq!(resp["countsByType"]["document"], json!(1));

    // filterOptionCounts：单 issue（todo/medium）计数。
    let foc = &resp["filterOptionCounts"];
    assert_eq!(foc["status"]["todo"], json!(1), "facet status.todo");
    assert_eq!(foc["priority"]["medium"], json!(1), "facet priority.medium");
    assert_eq!(
        foc["updatedWithin"]["24h"],
        json!(1),
        "facet updatedWithin.24h"
    );

    // scope=comments only: still surfaces (anySearchMatch is scope-independent),
    // but the match is comment-derived.
    let (_, body) = send(
        &app,
        &board,
        "GET",
        &format!(
            "/companies/{}/search?q=parrotsearch&scope=comments",
            company_a
        ),
    )
    .await;
    let resp = parse(&body);
    assert_eq!(resp["scope"], json!("comments"));
    assert_eq!(resp["results"].as_array().unwrap().len(), 1);

    // scope=issues only: comment/document matches excluded → empty.
    let (_, body) = send(
        &app,
        &board,
        "GET",
        &format!(
            "/companies/{}/search?q=parrotsearch&scope=issues",
            company_a
        ),
    )
    .await;
    let resp = parse(&body);
    assert_eq!(resp["scope"], json!("issues"));
    assert_eq!(
        resp["results"].as_array().unwrap().len(),
        0,
        "scope=issues excludes comment/doc-only matches"
    );

    // Cleanup.
    let _ = sqlx::query("DELETE FROM issue_documents WHERE company_id = $1")
        .bind(company_a)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM documents WHERE company_id = $1")
        .bind(company_a)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM issue_comments WHERE company_id = $1")
        .bind(company_a)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
        .bind(company_a)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM agents WHERE company_id = $1")
        .bind(company_a)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_a)
        .execute(&pool)
        .await;
}

/// #4C.3 Company Search acceptance — fuzzy identifier match via pg_trgm similarity
/// (§4C.3 slice 5). A typo'd query `loginflow` surfaces an issue whose identifier
/// `LGNFLW-1` is not a literal substring match but exceeds the 0.45 similarity
/// threshold (aligns Paperclip `fuzzyIdentifierMatch`).
#[tokio::test]
async fn company_search_fuzzy_identifier_matches_paperclip() {
    let pool = connect_and_migrate().await;
    let user = Uuid::new_v4();
    let company_a = Uuid::new_v4();
    let prefix_a = format!("FZ{}", &company_a.simple().to_string()[..8]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_a)
        .bind("Fuzzy Co A")
        .bind(&prefix_a)
        .execute(&pool)
        .await
        .expect("insert company");

    // Identifier `LOGINFLOW-{h}`（h=company uuid 首 hex）对 typo 查询 `loginflw` 的
    // pg_trgm 相似度 ≈0.5（≥0.45），且 `loginflw` 不是其字面子串 → 纯模糊命中路径。
    let fuzzy_ident = format!("LOGINFLOW-{}", &company_a.simple().to_string()[..1]);
    let _ = sqlx::query("DELETE FROM issues WHERE identifier = $1")
        .bind(&fuzzy_ident)
        .execute(&pool)
        .await
        .expect("pre-delete leftover identifier");
    let issue_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, identifier, status, priority, description) \
         VALUES ($1, $2, $3, $4, 'todo', 'medium', $5)",
    )
    .bind(issue_id)
    .bind(company_a)
    .bind("unrelated title with no keyword at all")
    .bind(&fuzzy_ident)
    .bind("unrelated description body")
    .execute(&pool)
    .await
    .expect("insert issue");

    let state = build_app_state(pool.clone())
        .await
        .expect("build_app_state");
    let app = company_routes().with_state(state);
    let board = board_actor(user, company_a);

    // typo 查询 "loginflw" 不字面命中 "LOGINFLOW-1"，但模糊相似度应浮出。
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search?q=loginflw", company_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let resp = parse(&body);
    let results = resp["results"].as_array().expect("results array");
    assert_eq!(
        results.len(),
        1,
        "fuzzy identifier match surfaces the issue"
    );
    let mf: Vec<String> = results[0]["matchedFields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(
        mf.contains(&"identifier".to_string()),
        "matchedFields includes identifier via fuzzy: {:?}",
        mf
    );

    // 短查询（<4 字符）不触发模糊匹配；"xy" 亦无字面命中。
    let (_, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search?q=xy", company_a),
    )
    .await;
    let resp = parse(&body);
    assert_eq!(
        resp["results"].as_array().unwrap().len(),
        0,
        "short query <4 chars disables fuzzy"
    );

    // Cleanup.
    let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
        .bind(company_a)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_a)
        .execute(&pool)
        .await;
}

/// #4C.3 Company Search acceptance — agents/projects scope in the main search
/// (§4C.3 slice 8). scope=agents returns agent results, scope=projects returns
/// project results (archived excluded), scope=all appends both after issues.
#[tokio::test]
async fn company_search_agents_projects_matches_paperclip() {
    let pool = connect_and_migrate().await;
    let user = Uuid::new_v4();
    let company_a = Uuid::new_v4();
    let prefix_a = format!("AP{}", &company_a.simple().to_string()[..8]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_a)
        .bind("Agents Projects Co")
        .bind(&prefix_a)
        .execute(&pool)
        .await
        .expect("insert company");

    // Agent matching "researcher".
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, role, status) VALUES ($1, $2, $3, $4, 'idle')",
    )
    .bind(agent_id)
    .bind(company_a)
    .bind("Sales Researcher")
    .bind("researcher")
    .execute(&pool)
    .await
    .expect("insert agent");

    // Project matching "research" (name) + one archived project that must NOT surface.
    let project_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects (id, company_id, name, description, status) \
         VALUES ($1, $2, $3, $4, 'in_progress')",
    )
    .bind(project_id)
    .bind(company_a)
    .bind("Research Portal")
    .bind("central research hub for the team")
    .execute(&pool)
    .await
    .expect("insert project");
    let archived_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects (id, company_id, name, description, status, archived_at) \
         VALUES ($1, $2, $3, $4, 'done', NOW())",
    )
    .bind(archived_id)
    .bind(company_a)
    .bind("Archived Research")
    .bind("old research notes")
    .execute(&pool)
    .await
    .expect("insert archived project");

    let state = build_app_state(pool.clone())
        .await
        .expect("build_app_state");
    let app = company_routes().with_state(state);
    let board = board_actor(user, company_a);

    // scope=agents: only agents.
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search?q=researcher&scope=agents", company_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let resp = parse(&body);
    assert_eq!(resp["scope"], json!("agents"));
    let results = resp["results"].as_array().expect("results array");
    assert_eq!(results.len(), 1, "one agent surfaces");
    assert_eq!(results[0]["type"], json!("agent"));
    assert_eq!(results[0]["title"], json!("Sales Researcher"));
    assert_eq!(
        results[0]["href"],
        json!(format!("/company/agents/{}", agent_id))
    );
    assert_eq!(results[0]["matchedFields"][0], json!("agent"));
    assert_eq!(resp["countsByType"]["agent"], json!(1));
    assert!(results[0]["snippet"].is_string(), "agent snippet set");

    // scope=projects: archived excluded.
    let (_, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search?q=research&scope=projects", company_a),
    )
    .await;
    let resp = parse(&body);
    assert_eq!(resp["scope"], json!("projects"));
    let results = resp["results"].as_array().expect("results array");
    assert_eq!(results.len(), 1, "one project surfaces; archived excluded");
    assert_eq!(results[0]["type"], json!("project"));
    assert_eq!(results[0]["title"], json!("Research Portal"));
    assert_eq!(
        results[0]["href"],
        json!(format!("/company/projects/{}", project_id))
    );
    assert_eq!(results[0]["matchedFields"][0], json!("project"));
    assert_eq!(resp["countsByType"]["project"], json!(1));

    // scope=all: agent + project appended after issues (no issue matches "research").
    let (_, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search?q=research", company_a),
    )
    .await;
    let resp = parse(&body);
    let results = resp["results"].as_array().expect("results array");
    let types: Vec<String> = results
        .iter()
        .map(|r| r["type"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        types,
        vec!["project", "agent"],
        "scope=all globally sorts project+agent by relevance"
    );
    assert_eq!(resp["countsByType"]["agent"], json!(1));
    assert_eq!(resp["countsByType"]["project"], json!(1));

    // Cleanup.
    let _ = sqlx::query("DELETE FROM projects WHERE company_id = $1")
        .bind(company_a)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM agents WHERE company_id = $1")
        .bind(company_a)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_a)
        .execute(&pool)
        .await;
}

/// #4C.3 Company Search acceptance — fuzzy title match via levenshtein_less_equal
/// (§4C.3 slice 9). Typo query `serach` (1 edit from "search", word length >= 6
/// -> max 2 edits) surfaces an issue whose title contains "Search" even though
/// `serach` is not a literal substring. Requires fuzzystrmatch extension.
#[tokio::test]
async fn company_search_fuzzy_title_matches_paperclip() {
    let pool = connect_and_migrate().await;
    let user = Uuid::new_v4();
    let company_a = Uuid::new_v4();
    let prefix_a = format!("FT{}", &company_a.simple().to_string()[..8]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_a)
        .bind("Fuzzy Title Co")
        .bind(&prefix_a)
        .execute(&pool)
        .await
        .expect("insert company");

    let issue_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, identifier, status, priority, description) \
         VALUES ($1, $2, $3, $4, 'todo', 'medium', $5)",
    )
    .bind(issue_id)
    .bind(company_a)
    .bind("Search results broken on mobile")
    .bind(format!("{}-1", prefix_a))
    .bind("unrelated description body")
    .execute(&pool)
    .await
    .expect("insert issue");

    let state = build_app_state(pool.clone())
        .await
        .expect("build_app_state");
    let app = company_routes().with_state(state);
    let board = board_actor(user, company_a);

    // `serach` 不是 "Search results..." 的字面子串，但 levenshtein("serach","search")=1
    // 且词长 6 >= FUZZY_PAIR_LONG_LENGTH → 允许 2 编辑 → 命中标题模糊。
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search?q=serach", company_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let resp = parse(&body);
    let results = resp["results"].as_array().expect("results array");
    assert_eq!(results.len(), 1, "fuzzy title match surfaces the issue");
    let mf: Vec<String> = results[0]["matchedFields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(
        mf.contains(&"title".to_string()),
        "matchedFields includes title via fuzzy: {:?}",
        mf
    );
    assert!(
        results[0]["snippet"].is_string(),
        "title snippet set for fuzzy match"
    );
    assert!(
        results[0]["snippet"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("search"),
        "snippet text from title: {:?}",
        results[0]["snippet"]
    );

    // Cleanup.
    let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
        .bind(company_a)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_a)
        .execute(&pool)
        .await;
}
