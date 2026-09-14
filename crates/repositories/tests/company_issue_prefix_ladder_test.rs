//! Regression coverage for the company issue-prefix allocation ladder.
//!
//! Companies sharing a 3-letter base allocate prefixes by appending one `A` per
//! retry: `EEG`, `EEGA`, `EEGAA`, ... The 9th company therefore needs an
//! 11-character prefix. While the column was `VARCHAR(10)` that insert failed
//! with SQLSTATE 22001, which the allocator did not recognize as a prefix
//! collision, so company creation returned HTTP 500 instead of advancing.
//!
//! See `migrations/20260913000001_widen_company_issue_prefix.sql`.

use models::CreateCompanyInput;
use repositories::company_repository::CompanyRepository;
use sqlx::PgPool;
use uuid::Uuid;

/// Chooses a 3-letter base that no company has claimed yet, so the run starts
/// from the bottom of the ladder regardless of earlier test debris.
async fn unused_base(pool: &PgPool) -> String {
    for _ in 0..20 {
        let hex = Uuid::new_v4().simple().to_string();
        let base: String = hex[..3]
            .chars()
            .map(|c| (b'A' + (c.to_digit(16).unwrap() as u8)) as char)
            .collect();
        let taken: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM companies WHERE issue_prefix = $1)")
                .bind(&base)
                .fetch_one(pool)
                .await
                .expect("probe issue prefix");
        if !taken {
            return base;
        }
    }
    panic!("could not find an unused issue prefix base");
}

fn company_input(name: &str) -> CreateCompanyInput {
    CreateCompanyInput {
        name: name.to_string(),
        description: None,
        issue_prefix: None,
        budget_monthly_cents: None,
        attachment_max_bytes: None,
        default_responsible_user_id: None,
        require_board_approval_for_new_agents: None,
    }
}

#[tokio::test]
async fn prefix_ladder_allocates_past_ten_characters() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping company issue prefix ladder test: DATABASE_URL is not set");
        return;
    };
    let pool = PgPool::connect(&database_url).await.expect("connect database");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");

    let repository = CompanyRepository::new(pool.clone());
    let base = unused_base(&pool).await;
    // The allocator derives the base from the first three alphabetic characters
    // of the name, so the generated base must lead the name.
    let name = format!("{base} Ladder Co");
    let creator = Uuid::new_v4();

    // Twelve same-base companies: attempts 9..12 need 11..14 character prefixes.
    let mut created = Vec::new();
    let mut prefixes = Vec::new();
    for attempt in 1..=12 {
        let company = repository
            .create(company_input(&name), creator)
            .await
            .unwrap_or_else(|error| {
                panic!("company {attempt} of the {base} ladder must be created: {error}")
            });
        prefixes.push(company.issue_prefix);
        created.push(company.id);
    }

    for (index, prefix) in prefixes.iter().enumerate() {
        let expected = format!("{base}{}", "A".repeat(index));
        assert_eq!(
            prefix, &expected,
            "attempt {} should allocate the next rung of the ladder",
            index + 1
        );
    }
    assert_eq!(
        prefixes[8].len(),
        11,
        "the 9th company must allocate a prefix longer than the old VARCHAR(10) limit"
    );

    sqlx::query("DELETE FROM company_memberships WHERE company_id = ANY($1)")
        .bind(&created)
        .execute(&pool)
        .await
        .expect("clean up ladder memberships");
    sqlx::query("DELETE FROM companies WHERE id = ANY($1)")
        .bind(&created)
        .execute(&pool)
        .await
        .expect("clean up ladder companies");
}
