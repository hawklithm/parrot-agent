# Handoff — parrot vs paperclip parity

**Date**: 2026-09-22
**Repo**: `/Users/adazhao/workspace/parrot/parrot-agent` (branch `parrot-agent`, HEAD `76fdb43`)
**Baseline**: `/Users/adazhao/workspace/paperclip` (TypeScript). Paperclip is the behavioral ground truth; inventing behavior is prohibited.

---

## 0. Current state — build is GREEN

`cargo build -p parrot-server` → **EXIT=0**, `Finished dev profile` in 5m06s.
Log: `/tmp/build.log`. 3 warnings, all pre-existing dead code in `crates/api/src/routes/tools.rs` (`mcp_http_request`, `mcp_http_request_with_config`, `mcp_stdio_request` never used).

**Nothing is committed.** All work below is uncommitted working-tree state.

> Pre-existing quirk: a build can exit 137 (SIGKILL/OOM during linking). Re-run; it is not a code problem.

---

## 1. What was completed and verified this session

### 1.1 B1 — skill-policy gateway migration (compile blocking, DONE)

A subagent rewrote the skill-policy model (`SkillPolicyEvaluateInput` / `SkillPolicyDecision` / `SkillPolicyDecisionReason` across 8 actions: `skills.create|import|install|edit|update|test|reset|remove`) and replaced the `PolicyDecision`/`DenialType` types, but left callers unmigrated. Three fixes landed:

**a) `crates/services/src/lib.rs:251-253`** — stale re-export removed (`DenialType`, `PolicyDecision` no longer exist).

**b) `crates/api/src/errors.rs`** — new variant so Paperclip's denial body survives `AppError`:
- `:56-60` — `AppError::Rendered { status: StatusCode, body: serde_json::Value }`
- `:263-265` — `IntoResponse` arm returns `(status, Json(body))` verbatim (early `return`, so no `unreachable!()` needed)
- `:313` — `variant_name()` arm

Rationale: Paperclip denials carry `code` / `reason` / `remediation` (`company-skills.ts:134-141`); `AppError`'s plain `String` variants drop them, which would strip the remediation hint the UI shows.

**c) `crates/api/src/routes/skill_policy.rs`** — added:
- `:405-408` — `impl From<PolicyError> for AppError` (`PolicyError = (StatusCode, Json<Value>)` at `:53`). Lets `skills.rs` handlers stay on `Result<_, AppError>` while reusing the shared gateway.
- `:418-455` — `pub(crate) async fn skill_policy_resource(...)`, mirroring Paperclip `skillPolicyResource` (`company-skills.ts:164-181`): reads the stored row, request values override `sourceLocator`/`key`/`sourceType`.
- `:457-460` — `fn as_non_empty` (Paperclip `asString` semantics).
- Needed imports added at `:31-37` (`normalize_skill_policy_source_locator`, `normalize_skill_policy_source_type`).

**d) `crates/api/src/routes/skills.rs`** — deleted local `enforce_skill_policy` + `actor_policy_role`; all 7 call sites now delegate to the shared gateway with Paperclip's true action vocabulary:

| line | handler | action | resource |
|---|---|---|---|
| 382 | `fork_company_skill` | `Create` | `skillPolicyResource({companyId, skillId})` |
| 565 | `import_company_skill` | `Import` | `{skillKey, sourceType: workspace}` |
| 596 | `update_company_skill` | `Edit` | `skillPolicyResource` |
| 629 | `create_skill_version` | `Create` | `skillPolicyResource` |
| 666 | `create_skill_test_run` | `Test` | `skillPolicyResource` |
| 713 | `create_company_skill` | `Create` | `{skillKey, sourceType: generated}` |
| 744 | `install_skill_catalog` | `Install` | `{sourceType: catalog}` |

Paperclip evidence: fork `:825` → `skills.create`; update `:422` → `skills.edit`; versions `:760` → `skills.create`; test-runs `:584` → `skills.test`; create-local `:1013` → `skills.create` + generated; install-catalog `:1183` → `skills.install` + catalog.

**Pre-existing deviation corrected**: local `update_company_skill` used action `"create"` (wrong; Paperclip is `skills.edit`) and `create_skill_test_run` used `"create"` (Paperclip is `skills.test`). The old local model had no `edit`/`test`, so these could not be expressed before.

Imports added to `skills.rs:15-17` (`normalize_skill_policy_source_type`, `SkillPolicyAction`, `SkillPolicyEvaluationResource`).

### 1.2 Earlier this session, verified end-to-end

- **Group A — three skills endpoints**: migration 93, `crates/repositories/src/skill_inventory.rs`, `skill_version_object`/`skill_test_run_object`, `skill_registry_service(_impl).rs`, 3 handlers at `skills.rs:609`, heartbeat hook, `id`/`hidden_at` plumbing. 197 tests green.
- **Two heartbeat defects**: `cancel_run` deadlock (double-locked `children`; now drops child after taking stdio, polls `try_wait`); cancellation recorded as `failed` (now writes terminal state before killing).
- **B2 document membership**: `UpdateDocumentMembershipInput` + `update_document` in `resource_membership_service.rs`; route `PUT /companies/:company_id/resource-memberships/me/documents/:document_id`. Semantics: row exists === starred; unstar is DELETE (no join/leave axis). HTTP-verified: star/idempotent/unstar/no-row 200, unknown doc 404, cross-company 404, missing `starred` 400, unknown key 400.
- **B4 browse-project**: `POST /companies/:company_id/skills/browse-project` + new `normalize_project_browse_path` (deliberately NOT reusing `normalize_project_skill_path`, which maps `skill.md` → `"."`). HTTP-verified incl. 250-entry truncation and all error branches.
- **Migration 94** `94_project_workspace_fields.sql` — fixes a pre-existing bug: `project_workspaces` had only 8 columns while `project_repository.rs:266` INSERTs 17 and `ProjectWorkspace` has 20 fields, so **any** workspace create/read failed. Added 12 columns + `cwd` backfill + 3 indexes; idempotent.
- **Body rejection middleware**: `media_type_rejection.rs` → `body_rejection.rs`, `normalize_unsupported_media_type` → `normalize_body_rejection_status`; now normalizes 415 **and** 422. Discriminator: axum extractor rejections are `text/plain`, all `AppError` responses are `application/json` (always via `Json`), so handler-intended 422s are untouched. Verified: malformed JSON → 400, missing field → 400, handler 422 passes through.

---

## 2. Known gaps introduced by the B1 migration (NOT verified, NOT fixed)

These are consequences of the migration above and should be resolved before the next parity claim.

1. **`PlatformInvariant` is now dead.** `SkillPolicyDecisionReason::PlatformInvariant` (`skill_policy_service.rs:149`) is declared but never produced by `evaluate`. In Paperclip, `assertCanMutateCompanySkills` (`company-skills.ts:200-232`) first runs the platform `access.decide({action: "skill_config:update", ...})` layer and throws `forbidden(...)` for any denial except `deny_no_grant`/`deny_missing_consent`/`deny_missing_grant`. Parrot's `enforce_skill_policy` (`skill_policy.rs:366`) goes straight to the company policy decision. **Parrot's platform-invariant layer is missing.**
2. **Protected-skill 403 is no longer enforced at the route layer.** The old model had `PROTECTED_SKILLS = ["system","internal","core","platform"]` + `is_protected_skill()` (HEAD `skill_policy_service.rs:46,91`, denial at `:229-232`). HEAD's evaluate matched on `skill`/`source`/`skill_key` only, so this check was **already ineffective** — which is why `crates/server/tests/skills_http_parity_test.rs:344` still passes. The test asserts `POST /skills/import` with `{"name":"system","skillKey":"system"}` → 403. Enforcing it properly needs the platform layer from item 1.
3. **`skill_key` for import/create is a weak input mapping.** Paperclip's import route parses `source` (`skillImportPolicyResource`, `:183-192`) and derives `sourceType` from git / https / workspace. Parrot's `import_company_skill` reads `payload.skillKey|key|name` and hardcodes `sourceType: "workspace"`; `create_company_skill` reads `payload.name` as `skillKey`. Arbitrary, not Paperclip-derived. (Paperclip's own create-local path passes only `{sourceType:"generated"}` — no key.)
4. **`install_skill_catalog` has no body.** Paperclip passes `sourceLocator: req.body.catalogSkillId` (`:1183`); Parrot's handler takes no JSON body and installs the whole catalog, so `source_locator` is `None`. Rules selecting on `sourceLocators` will not match.
5. **`skill_policy_resource` swallows lookup errors.** Paperclip's `getById` returns `null` on miss; Parrot's `get_skill_by_id` raises `NotFound`. The helper uses `.ok()` so a miss degrades to "request values only" — but it also swallows genuine DB errors. Handler-side 404 stays authoritative.
6. **`skills.rs` handlers still use `.map_err(|e| AppError::InternalServerError(e.to_string()))`** in several places (e.g. `import_company_skill`, `create_company_skill`, `install_skill_catalog`). This collapses `ServiceError` mapping into a flat 500; `AppError::from` (via `errors.rs:143`) is correct. Pre-existing pattern, not introduced here — but it is wrong.

---

## 3. Unfinished work (stopped on user instruction)

- **B7 onboarding-seed** — files exist but are **not wired**: `crates/services/src/onboarding_seed_service.rs` (823 lines), `crates/api/src/routes/onboarding_seed.rs` (209 lines). `grep onboarding_seed` in `services/src/lib.rs`, `routes/mod.rs`, `app_state.rs` is empty. Needs `pub mod` ×2 + `.merge(...)`. Also reported: `parse_seed_mission` should split on `/\r?\n/` to avoid eating a stray `\r` mid-string.
- **B3 skill rename** — unconfirmed whether it landed (`grep rename` in `skills.rs` / `skill_registry_service(_impl).rs`). Blockers: Parrot has no managed-skills root (`agent_service.rs:494` hardcodes `"managedRootPath": ""`; `managed_local`/`managedRoot`/`__runtime__` all empty) and `company_skills` has no `sourceKind` column.
- **B5 stalled-review-decision** — 12 unverified lines in `issue_thread_interaction_service.rs`. Decide path, 404 shape, 409 race all unverified. Do NOT reuse `stalled_review_decisions_service.rs` (that is the auto-detector querying a non-existent `reviews` table). Parrot has no `list_review_attention`.
- **B6 adapter login-sessions** — **no file produced**. `crates/services/src/adapter_login_session_service.rs` does not exist; `adapters.rs` has no login-session trace. Table `adapter_auth_sessions` does exist.
- **C1/C2** — both scout attempts produced **no output file** (`local://groupc-findings.md` never written; `GroupCScout` transcript shows reading only). Claims need fresh investigation. Note one C claim pointed at `crates/api/src/routes/claude_local_adapter.rs`, which **does not exist**; the real file is `crates/services/src/adapters/claude_local_adapter.rs:278` and its `test_environment` has 4 real checks (not fake).
- **Regression suite** — not re-run after the B1 migration. Command set in §5.
- **Commit** — user has not approved. Scope: the git status in §6, **excluding** `.env`, `MCP_PARITY_CHECKLIST.md`, `crates/server/data/`.
- 32 server test files still use leaky `let _ = DELETE FROM companies`. Out of scope.
- Pre-existing clippy error `crates/services/src/plan_review_context_service.rs:385` (`absurd_extreme_comparisons`). Unrelated.

---

## 4. Environment / command reference

```bash
# psql
export PATH="/Library/PostgreSQL/18/bin:$PATH"
# DB
postgres://postgres:postgres@localhost:5432/parrot_agent_dev

# build (crate name is parrot-server / parrot_server — `-p server` FAILS)
cd /Users/adazhao/workspace/parrot/parrot-agent
env -u CI bash -lc 'cargo build -p parrot-server > /tmp/build.log 2>&1; echo "EXIT=$?"'

# tests — `env -u CI` is REQUIRED (harness CI=true breaks telemetry_service::tests::test_telemetry_enabled_by_default)
# integration tests need DATABASE_URL or crates/server/tests/common/mod.rs:27 panics
env -u CI DATABASE_URL="postgres://postgres:postgres@localhost:5432/parrot_agent_dev" \
  bash -lc 'cargo test -p ...'
```

Cargo must run serially. `hub start`'s `env` param is inert — use bash `env VAR=… binary`.

Server launch (confirm `lsof -ti :3100` is **empty** first, or the new process silently dies with `AddrInUse` and the old binary keeps serving):
```bash
nohup env DATABASE_URL="postgres://postgres:postgres@localhost:5432/parrot_agent_dev" \
  PORT=3100 ./target/debug/parrot-server > /tmp/parrot-server.log 2>&1 &
```

Probing: without a session, endpoints fall through to the **local-implicit dev resolver** and return 200 even unauthenticated. To probe as a real actor, a valid session already exists:
`Cookie: default-session=probe-token-abc123`.

Frontend listens on `[::1]:5173` → use `http://localhost:5173/`.

---

## 5. Regression commands (run after any further change)

```bash
env -u CI DATABASE_URL=… cargo test -p services --lib heartbeat
env -u CI DATABASE_URL=… cargo test -p repositories --lib skill_inventory
env -u CI DATABASE_URL=… cargo test -p api --lib routes::
env -u CI DATABASE_URL=… cargo test -p parrot-server --test skills_http_parity_test
env -u CI DATABASE_URL=… cargo test -p parrot-server --test agent_skills_http_parity_test
env -u CI DATABASE_URL=… cargo test -p parrot-server --test skill_policy_http_parity_test
env -u CI DATABASE_URL=… cargo test -p parrot-server --test tool_gateway_action_claim_test
```

Fixtures (live in `parrot_agent_dev`):
```
CID=1f607874-0721-4274-bbc8-64fb445a1c7c          company 'marcus'
UID_USER=f7232cf5-96e1-4c3d-ac22-3d196b5a805b     owner member
PID=bc19bb89-459d-463d-b2e4-04e2b9c085e8          project 'Onboarding'
DID=8eee10f0-8e90-4ebd-894b-939ccc03d533          document
WID=5368b506-8966-458d-8a0f-d48467dff972          workspace, local_path /tmp/browse-fixture
WID_REMOTE=afc07814-967b-4f07-b15b-0dc26babae53   remote_managed
WID_NOCWD=9afdf029-57e4-4350-92f1-004c07c0f074    no cwd
AID=9d9a59c6-9d6d-4b68-89c1-efb01bfbd957          agent 'Summarizer'
SID=0d13dfd1-188e-45d3-b5d4-f4aba9f9e6d1          skill 'Summarize status'
B=http://127.0.0.1:3100/api
```

---

## 6. Working-tree state

`git status --short` (33 modified, 7 untracked, 0 staged):

```
 M .env                                                    ← PRE-EXISTING DIRTY, do not commit
 M MCP_PARITY_CHECKLIST.md                                  ← PRE-EXISTING DIRTY, do not commit
 M crates/api/src/app_state.rs                             body_rejection wiring
 M crates/api/src/errors.rs                                B1: AppError::Rendered
 M crates/api/src/middleware/mod.rs                        module rename
 M crates/api/src/routes/cases.rs
 M crates/api/src/routes/issues.rs                         B5 partial (unverified)
 M crates/api/src/routes/llms.rs                           SkillPolicy
 M crates/api/src/routes/resource_memberships.rs           B2
 M crates/api/src/routes/skill_policy.rs                   SkillPolicy + B1 bridge
 M crates/api/src/routes/skills.rs                         B4 + B1 migration
 M crates/api/src/routes/tools.rs
 M crates/models/src/issue.rs
 D crates/repositories/src/company_skill_policy_repository.rs   deleted by SkillPolicy; no references remain
 M crates/repositories/src/lib.rs
 M crates/repositories/src/pg_issue_repository.rs
 M crates/repositories/src/pg_skill_repository.rs
 M crates/repositories/src/skill_repository.rs             +51 (rename)
 M crates/server/src/lib.rs
 M crates/server/tests/*.rs                                (6 test files)
 M crates/services/src/heartbeat_service.rs
 M crates/services/src/issue_service_complete.rs
 M crates/services/src/issue_thread_interaction_service.rs  B5 partial (unverified)
 M crates/services/src/lib.rs                              B1: stale re-export removed
 M crates/services/src/resource_membership_service.rs       B2
 M crates/services/src/skill_policy_service.rs             rewritten (+1887)
 M crates/services/src/skill_registry_service.rs           +59
 M crates/services/src/skill_registry_service_impl.rs      rewritten (+2084)
?? crates/api/src/middleware/body_rejection.rs
?? crates/api/src/routes/onboarding_seed.rs                209 lines, NOT WIRED
?? crates/repositories/src/skill_inventory.rs
?? crates/server/data/                                     do not commit
?? crates/services/src/onboarding_seed_service.rs          823 lines, NOT WIRED
?? migrations/93_skill_test_run_parity.sql
?? migrations/94_project_workspace_fields.sql
```

Outer workspace `/Users/adazhao/workspace/parrot`: `M parrot-agent`, `M parrot-web-ui`.

---

## 7. Schema notes

```
project_workspaces   20 cols after migration 94 (id, project_id, name, config, is_primary, created_at,
                     updated_at, company_id, source_type, cwd, repo_url, repo_ref, default_ref,
                     visibility, setup_command, cleanup_command, remote_provider, remote_workspace_ref,
                     shared_workspace_key, metadata)
document_memberships id, company_id, document_id, user_id, starred_at, created_at, updated_at
                     UNIQUE(company_id, user_id, document_id)   -- NO state column
activity_logs        id, company_id, event_type, actor_type, actor_id, resource_type, resource_id,
                     metadata, created_at, run_id, agent_id    -- event_type/resource_type, NOT action/entity_type
company_memberships  ... membership_role ...                    -- NOT `role`
auth_sessions        id, user_id, token, expires_at, created_at, last_accessed_at  (token plaintext)
company_skill_policies  id, company_id, policy jsonb, version int, created_at, updated_at
company_skills       has is_paperclip_managed/source_type/source_locator/key/slug/markdown;
                     NO sourceKind column, NO current_version_id column
skill_versions       revision_number int NOT NULL, label text, file_inventory jsonb NOT NULL DEFAULT '[]'
adapter_auth_sessions  id, company_id, environment_id, adapter_type, started_by_user_id,
                     provider_lease_id, status, expires_at, promotion_expires_at, finished_at,
                     failure_reason, created_at, updated_at
company_onboarding_seeds  id, company_id, revision, mission, agent_name, agent_role,
                     first_task_title, first_task_details, goal_id, agent_id, issue_id,
                     applied_at, created_at, updated_at
issues               has hidden_at, execution_run_id, checkout_run_id, execution_locked_at
NO reviews table
```

---

## 8. Rules that cost time to learn

- **Never** `AppError::InternalServerError(e.to_string())` for a `ServiceError`. Use `?` or `.map_err(AppError::from)` — `errors.rs:143`'s `From<ServiceError>` is the only correct mapping (see §2 item 6 for remaining offenders).
- **Subagents must not run `cargo`** — concurrent half-finished edits collide. Main compiles once. Five agents hit budget exhaustion in the prior session, leaving unwired code (B7).
- Querystring params are camelCase.
- Do not re-investigate these dead ends: the `POST /companies/:id/skills` 500 was stale fixture IDs after a DB reset (FK violation); `/companies/:id/adapters/:type/models`, `/issues/:id/file-resources/list`, `/cases/:id/events`, `/skills/catalog/files`, `_plugins/:id/ui/*file_path` all exist; `crates/api/src/routes/claude_local_adapter.rs` does not exist; the "41 missing route declarations" claim contained regex artifacts (`${qs`, `${query`).
