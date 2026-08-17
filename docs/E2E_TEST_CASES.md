# Parrot Agent - E2E Test Cases

**Version**: v2.0  
**Last Updated**: 2026-08-17  
**Status**: Active

---

## Document Overview

本文档定义了 Parrot Agent 系统的端到端测试用例,包含:
- **标准化测试模板**:每个用例包含 ID、优先级、验证条件、失败条件
- **可执行的验证步骤**:明确的断言条件
- **自动化脚本参考**:关键用例的 Playwright 脚本示例

---

## Test Environment

### Prerequisites

| Component | Requirement |
|-----------|-------------|
| Backend | `http://localhost:3100` (running) |
| Frontend | `http://localhost:5173` (running) |
| Database | PostgreSQL with migrations applied |
| Test Data | Clean state via `cargo run --bin reset_database` |
| Browser | Chrome/Edge (latest), localStorage cleared |

### Default Test Data

```yaml
company_id: "00000000-0000-0000-0000-000000000000"
company_name: "Default Company"
board_user_id: "48592512-465a-4ed7-9b12-ca554ee636e8"
board_user_email: "board@local.dev"
require_board_approval: false
```

---

## Test Case Template

每个测试用例遵循以下结构:

```markdown
### [TC-XXX] Test Case Title

**Priority**: P0/P1/P2  
**Category**: Authentication | Agent Management | Issues | ...  
**Dependencies**: [TC-001], [TC-002]  
**Automation**: ✅ Automated | ⏳ Manual | 🔄 Semi-automated

#### Test Objective
Clear statement of what this test validates.

#### Preconditions
- Condition 1
- Condition 2

#### Test Steps
1. Action 1
2. Action 2
3. ...

#### Verification Criteria

**UI Checks**:
- [ ] UI element 1 is visible
- [ ] UI element 2 shows correct text

**Data Checks**:
- [ ] Database record exists: `SELECT ... WHERE ...`
- [ ] API response contains: `{ "field": "value" }`

**Functional Checks**:
- [ ] Behavior 1 occurs
- [ ] Behavior 2 does not occur

**Performance Checks** (if applicable):
- [ ] Response time < 500ms
- [ ] Page load < 2s

#### Failure Conditions
- Condition that indicates test failure

#### Rollback Steps
How to restore clean state after test.
```

---

## Priority Levels

| Priority | Description | SLA |
|----------|-------------|-----|
| **P0** | Critical path, blocks core functionality | Must pass before any release |
| **P1** | Important features, affects user experience | Must pass before major release |
| **P2** | Nice-to-have, edge cases | Best effort |

---

## Test Cases

## 1. Authentication & Authorization

### [TC-001] First Visit and Onboarding Flow

**Priority**: P0  
**Category**: Authentication  
**Dependencies**: None  
**Automation**: ✅ Automated

#### Test Objective
验证首次访问时,系统正确引导用户完成 onboarding 流程。

#### Preconditions
- Database reset: `cargo run --bin reset_database`
- Browser localStorage cleared: visit `/clear-storage.html`

#### Test Steps
1. Navigate to `http://localhost:5173/`
2. System should auto-redirect to onboarding page
3. Observe welcome screen

#### Verification Criteria

**UI Checks**:
- [ ] URL is `/onboarding` or shows onboarding wizard
- [ ] Page displays "Welcome to Parrot Agent" or similar heading
- [ ] "Create First Agent" or "Setup Company" button is visible

**Data Checks**:
- [ ] No auth session in localStorage: `localStorage.getItem('auth_token') === null`
- [ ] No cookies set for authentication

**Functional Checks**:
- [ ] Clicking "Create Agent" navigates to agent creation form
- [ ] Back button (if present) does not crash the app

#### Failure Conditions
- Infinite redirect loop
- Blank page / 404 error
- JavaScript console errors

#### Rollback Steps
Execute full database reset.

---

### [TC-002] Board User Login (Local Trusted Mode)

**Priority**: P0  
**Category**: Authentication  
**Dependencies**: [TC-001]  
**Automation**: ✅ Automated

#### Test Objective
验证在 `local_trusted` 模式下,board 用户可以成功登录。

#### Preconditions
- `DEPLOYMENT_MODE=local_trusted` in `.env`
- Database has board user: `48592512-465a-4ed7-9b12-ca554ee636e8`

#### Test Steps
1. Click "Login as Board User" button (or similar)
2. System authenticates as board user
3. User is redirected to dashboard

#### Verification Criteria

**UI Checks**:
- [ ] URL changes to `/` or `/dashboard`
- [ ] Top-right corner shows user email: "board@local.dev"
- [ ] Navigation sidebar is visible

**Data Checks**:
- [ ] Auth session exists in database:
  ```sql
  SELECT * FROM auth_sessions WHERE user_id = '48592512-465a-4ed7-9b12-ca554ee636e8' AND expires_at > NOW();
  ```
- [ ] Auth token stored in localStorage: `localStorage.getItem('auth_token') !== null`

**API Checks**:
- [ ] GET `/api/auth/me` returns 200 with user data:
  ```json
  {
    "id": "48592512-465a-4ed7-9b12-ca554ee636e8",
    oard@local.dev"
  }
  ```

**Functional Checks**:
- [ ] All navigation links are clickable
- [ ] No "unauthorized" errors in console

#### Failure Conditions
- Login button does nothing
- API returns 401/403
- Auth token not persisted

#### Rollback Steps
Clear localStorage: `localStorage.clear()`.

---

### [TC-003] Logout Flow

**Priority**: P1  
**Category**: Authentication  
**Dependencies**: [TC-002]  
**Automation**: ✅ Automated

#### Test Objective
验证用户可以成功登出,所有会话数据被清除。

#### Preconditions
- User is logged in ([TC-002] passed)

#### Test Steps
1. Click user avatar/menu in top-right
2. Click "Logout" button
3. Observe redirect to login page

#### Verification Criteria

**UI Checks**:
- [ ] URL is `/login` or `/onboarding`
- [ ] User email no longer visible in UI
- [ ] Navigation sidebar is hidden or shows public-only links

**Data Checks**:
- [ ] Auth token removed from localStorage: `localStorage.getItem('auth_token') === null`
- [ ] Session invalidated in database:
  ```sql
  SELECT * FROM auth_sessions WHERE token = '<token>' AND expires_at > NOW();
  -- Should return 0 rows
  ```

**API Checks**:
- [ ] GET `/api/auth/me` returns 401

#### Failure Conditions
- User still logged in after logout
- Auth token persists in storage
- Can still access protected routes

#### Rollback Steps
Force logout via `localStorage.clear()` and page refresh.

---

## 2. Agent Management

### [TC-101] View Agent List (Empty State)

**Priority**: P1  
**Category**: Agent Management  
**Dependencies**: [TC-002]  
**Automation**: ✅ Automated

#### Test Objective
验证用户可以查看 agent 列表,空状态显示正确的提示信息。

#### Preconditions
- User logged in
- No agents exist: database reset

#### Test Steps
1. Click "Agents" in navigation sidebar
2. Observe empty state UI

#### Verification Criteria

**UI Checks**:
- [ ] URL is `/agents`
- [ ] Empty state message displayed: "No agents yet" or similar
- [ ] "Create Agent" button is visible

**Data Checks**:
- [ ] API call to GET `/api/agents` returns empty array: `[]`
- [ ] No agents in database:
  ```sql
  SELECT COUNT(*) FROM agents WHERE company_id = '00000000-0000-0000-0000-000000000000';
  -- Should return 0
  ```

#### Failure Conditions
- 404 error
- Loading spinner never stops
- Console errors

#### Rollback Steps
None (read-only operation).

---

### [TC-102] Create New Agent (Happy Path)

**Priority**: P0  
**Category**: Agent Management  
**Dependencies**: [TC-002]  
**Automation**: ✅ Automated

#### Test Objective
验证用户可以成功创建一个新的 agent,所有字段正确保存。

#### Preconditions
- User logged in
- On `/agents` page

#### Test Steps
1. Click "New Agent" button
2. Fill in form:
   - **Name**: "TestAgent01"
   - **Role**: "Software Engineer"
   - **Description**: "Test agent for E2E validation"
3. Click "Create" button
4. Wait for creation to complete

#### Verification Criteria

**UI Checks**:
- [ ] Success toast/notification appears: "Agent created successfully"
- [ ] Redirected to agent detail page: `/agents/:id`
- [ ] Agent name "TestAgent01" displayed in header
- [ ] Status badge shows "active" or "ready"

**Data Checks**:
- [ ] Agent exists in database:
  ```sql
  SELECT * FROM agents WHERE name = 'TestAgent01' AND company_id = '00000000-0000-0000-0000-000000000000';
  ```
- [ ] Agent ID is a valid UUID
- [ ] `created_at` timestamp is recent (< 5 seconds ago)

**API Checks**:
- [ ] POST `/api/agents` returned 201 with agent data:
  ```json
  {
    "id": "<uuid>",
    "name": "TestAgent01",
    "role": "Software Engineer",
    "status": "active"
  }
  ```

**Functional Checks**:
- [ ] Agent appears in agent list: `/agents`
- [ ] Can navigate to agent detail page by clicking agent card

#### Failure Conditions
- Form validation errors with valid input
- API returns 500 error
- Agent not saved to database
- Duplicate agent created

#### Rollback Steps
Delete agent via UI or SQL:
```sql
DELETE FROM agents WHERE name = 'TestAgent01';
```

---

### [TC-103] Edit Agent Details

**Priority**: P1  
**Category**: Agent Management  
**Dependencies**: [TC-102]  
**Automation**: 🔄 Semi-automated

#### Test Objective
验证用户可以修改 agent 的详细信息。

#### Preconditions
- Agent "TestAgent01" exists ([TC-102] passed)
- On agent detail page: `/agents/:id`

#### Test Steps
1. Click "Edit" button
2. Modify fields:
   - **Name**: "TestAgent01_Updated"
   - **Description**: "Updated description"
3. Click "Save"

#### Verification Criteria

**UI Checks**:
- [ ] Success notification appears
- [ ] Page displays updated name: "TestAgent01_Updated"
- [ ] Updated description visible

**Data Checks**:
- [ ] Database updated:
  ```sql
  SELECT name, description, updated_at FROM agents WHERE id = '<agent_id>';
  -- name = 'TestAgent01_Updated'
  -- updated_at is recent
  ```

**API Checks**:
- [ ] PUT `/api/agents/:id` returned 200

#### Failure Conditions
- Changes not saved
- API returns error
- Old data still displayed after refresh

#### Rollback Steps
Revert changes via edit form or SQL UPDATE.

---

### [TC-104] Delete Agent

**Priority**: P1  
**Category**: Agent Management  
**Dependencies**: [TC-102]  
**Automation**: ⏳ Manual (requires confirmation dialog)

#### Test Objective
验证用户可以删除 agent,所有关联数据正确处理。

#### Preconditions
- Agent exists
- On agent detail page

#### Test Steps
1. Click "Delete" button
2. Confirm in dialog: "Are you sure?"
3. Agent deleted

#### Verification Criteria

**UI Checks**:
- [ ] Confirmation dialog appears
- [ ] After confirmation, redirected to `/agents`
- [ ] Deleted agent no longer in list

**Data Checks**:
- [ ] Agent marked as deleted or removed:
  ```sql
  SELECT * FROM agents WHERE id = '<agent_id>';
  -- Either deleted_at IS NOT NULL or 0 rows returned
  ```

**API Checks**:
- [ ] DELETE `/api/agents/:id` returned 200 or 204

#### Failure Conditions
- Agent not deleted from database
- Still visible in UI after deletion
- Related data (issues, tasks) orphaned

#### Rollback Steps
Recreate agent manually or restore from backup.

---

## 3. Issue Management

### [TC-201] View Issue List (Empty State)

**Priority**: P1  
**Category**: Issues  
**Dependencies**: [TC-002]  
**Automation**: ✅ Automated

#### Test Objective
验证用户可以查看任务列表,空状态显示正确。

#### Preconditions
- User logged in
- No issues exist

#### Test Steps
1. Navigate to `/issues`
2. Observe empty state

#### Verification Criteria

**UI Checks**:
- [ ] Empty state message: "No issues yet"
- [ ] "Create Issue" button visible

**Data Checks**:
- [ ] GET `/api/issues` returns `[]`
- [ ] Database has 0 issues:
  ```sql
  SELECT COUNT(*) FROM issues WHERE company_id = '00000000-0000-0000-0000-000000000000';
  ```

#### Failure Conditions
- Loading spinner never stops
- API error

#### Rollback Steps
None (read-only).

---

### [TC-202] Create New Issue - Manual Creation

**Priority**: P0  
**Category**: Issues  
**Dependencies**: [TC-002], [TC-102] (optional agent)  
**Automation**: ✅ Automated

#### Test Objective
验证用户可以手动创建任务,任务正确保存并可分配给 agent。

#### Preconditions
- User logged in
- (Optional) Agent exists for assignment

#### Test Steps
1. Navigate to `/issues`
2. Click "New Issue"
3. Fill form:
   - **Title**: "Implement user login feature"
   - **Description**: "Need JWT-based authentication"
   - **Priority**: "high"
   - **Assignee**: Select "TestAgent01" (if exists)
4. Click "Create"

#### Verification Criteria

**UI Checks**:
- [ ] Redirected to issue detail page: `/issues/:id`
- [ ] Issue title displayed: "Implement user login feature"
- [ ] Priority badge shows "High"
- [ ] Assignee shows "TestAgent01" (if assigned)
- [ ] Status is "open" or "pending"

**Data Checks**:
- [ ] Issue saved in database:
  ```sql
  SELECT * FROM issues WHERE title = 'Implement user login feature';
  ```
- [ ] Has valid UUID, company_id, created_at
- [ ] If assigned: `assignee_agent_id` = TestAgent01's ID

**API Checks**:
- [ ] POST `/api/issues` returned 201 with issue data

**Functional Checks**:
- [ ] Issue appears in `/issues` list
- [ ] If assigned to agent, agent's task queue updated (check `/agents/:id`)

#### Failure Conditions
- Issue not saved
- Assignee not linked
- Duplicate issues created on multiple clicks

#### Rollback Steps
Delete issue:
```sql
DELETE FROM issues WHERE title = 'Implement user login feature';
```

---

### [TC-203] View Issue Detail Page

**Priority**: P0  
**Category**: Issues  
**Dependencies**: [TC-202]  
**Automation**: ✅ Automated

#### Test Objective
验证任务详情页显示所有相关信息。

#### Preconditions
- Issue created ([TC-202] passed)

#### Test Steps
1. From `/issues` list, click on issue card
2. Observe detail page

#### Verification Criteria

**UI Checks**:
- [ ] URL is `/issues/:id`
- [ ] Title, description, priority, status visible
- [ ] Assignee (if any) displayed
- [ ] Created/updated timestamps shown
- [ ] Comment section visible
- [ ] Activity timeline visible

**Data Checks**:
- [ ] GET `/api/issues/:id` returns full issue data

**Functional Checks**:
- [ ] Can add comment (if implemented)
- [ ] Can change status (if permissions allow)

#### Failure Conditions
- 404 error for valid issue ID
- Missing fields in UI
- Stale data (not reflecting recent updates)

#### Rollback Steps
None (read-only).

---

### [TC-204] Update Issue Status

**Priority**: P0  
**Category**: Issues  
**Dependencies**: [TC-202]  
**Automation**: 🔄 Semi-automated

#### Test Objective
验证用户或 agent 可以更新任务状态。

#### Preconditions
- Issue exists with status "open"

#### Test Steps
1. On issue detail page
2. Change status dropdown to "in_progress"
3. Click "Save" or status auto-saves

#### Verification Criteria

**UI Checks**:
- [ ] Status badge updates to "In Progress"
- [ ] Activity timeline shows status change event

**Data Checks**:
- [ ] Database updated:
  ```sql
  SELECT status, updated_at FROM issues WHERE id = '<issue_id>';
  -- status = 'in_progress'
  ```

**API Checks**:
- [ ] PUT `/api/issues/:id` or PATCH returned 200

#### Failure Conditions
- Status not saved
- Optimistic UI update reverted on refresh

#### Rollback Steps
Set status back to "open" via UI or SQL.
\--

### [TC-205] Assign Issue to Agent

**Priority**: P0  
**Category**: Issues  
**Dependencies**: [TC-102], [TC-202]  
**Automation**: 🔄 Semi-automated

#### Test Objective
验证可以将任务分配给 agent,agent 收到通知并开始处理。

#### Preconditions
- Issue exists (unassigned or assigned to different agent)
- Agent "TestAgent01" exists

#### Test Steps
1. On issue detail page
2. Click "Assign" or select assignee dropdown
3. Select "TestAgent01"
4. Confirm assignment

#### Verification Criteria

**UI Checks**:
- [ ] Assignee field shows "TestAgent01"
- [ ] Agent avatar/icon appears

**Data Checks**:
- [ ] Database updated:
  ```sql
  SELECT assignee_agent_id FROM issues WHERE id = '<issue_id>';
  -- assignee_agent_id = TestAgent01's UUID
  ```

**Functional Checks**:
- [ ] Agent's task queue includes this issue (check `/agents/:id`)
- [ ] (If implemented) Agent receives notification/event

#### Failure Conditions
- Assignment not saved
- Agent not notified
- Issue not in agent's queue

#### Rollback Steps
Unassign issue via UI or SQL:
```sql
UPDATE issues SET assignee_agent_id = NULL WHERE id = '<issue_id>';
```

---

## 4. Advanced Scenarios

### [TC-301] Multi-Agent Collaboration

**Priority**: P2  
**Category**: Collaboration  
**Dependencies**: [TC-102] (multiple agents), [TC-202]  
**Automation**: ⏳ Manual

#### Test Objective
验证多个 agent 可以协作处理同一个复杂任务。

#### Preconditions
- 2+ agents exist
- Complex issue exists that requires collaboration

#### Test Steps
1. Create issue with description requiring multiple skills
2. Assign to Agent1
3. Agent1 delegates subtask to Agent2 (if system supports)
4. Monitor collaboration in activity timeline

#### Verification Criteria

**UI Checks**:
- [ ] Activity timeline shows collaboration events
- [ ] Both agents listed as contributors

**Functional Checks**:
- [ ] Subtasks created and assigned correctly
- [ ] Communication between agents logged

#### Failure Conditions
- Agents work in isolation, ignore each other
- Duplicate work performed

#### Rollback Steps
Delete issue and reset agents.

---

### [TC-302] Approval Workflow (Board Review)

**Priority**: P1  
**Category**: Approval  
**Dependencies**: [TC-002], [TC-102]  
**Automation**: ⏳ Manual

#### Test Objective
验证当 `require_board_approval_for_new_agents = true` 时,审批流程正常工作。

#### Preconditions
- Update company settings:
  ```sql
  UPDATE companies SET require_board_approval_for_new_agents = true WHERE id = '00000000-0000-0000-0000-000000000000';
  ```

#### Test Steps
1. Create new agent
2. Agent enters "pending_approval" status
3. Board user reviews and approves/rejects

#### Verification Criteria

**UI Checks**:
- [ ] Agent card shows "Pending Approval" badge
- [ ] Board user sees approval queue

**Data Checks**:
- [ ] Agent status is "pending_approval" in database

**Functional Checks**:
- [ ] Approved agent transitions to "active"
- [ ] Rejected agent deleted or marked inactive

#### Failure Conditions
- Agent auto-activates without approval
- Approval action has no effect

#### Rollback Steps
Reset company setting:
```sql
UPDATE companies SET require_board_approval_for_new_agents = false WHERE id = '...';
```

---

## 5. Performance Tests

### [TC-401] Agent List Load Performance

**Priority**: P1  
**Category**: Performance  
**Dependencies**: None  
**Automation**: ✅ Automated (via Playwright performance API)

#### Test Objective
验证在有大量 agents 的情况下,列表页加载性能符合要求。

#### Preconditions
- 100+ agents in database

#### Test Steps
1. Navigate to `/agents`
2. Measure page load time

#### Verification Criteria

**Performance Checks**:
- [ ] Initial page load (HTML) < 500ms
- [ ] API response time < 1s
- [ ] Full page render < 2s
- [ ] Pagination/virtualization implemented for large lists

#### Failure Conditions
- Page takes > 5s to load
- Browser freezes
- Out of memory errors

#### Rollback Steps
None.

---

### [TC-402] Issue Creation Under Load

**Priority**: P2  
**Category**: Performance  
**Dependencies**: None  
**Automation**: ✅ Automated (load testing tool)

#### Test Objective
验证系统可以处理并发的任务创建请求。

#### Preconditions
- Load testing tool configured (e.g., k6, Artillery)

#### Test Steps
1. Simulate 10 concurrent users creating issues
2. Monitor API response times and error rates

#### Verification Criteria

**Performance Checks**:
- [ ] 95th percentile response time < 1s
- [ ] Error rate < 1%
- [ ] No database deadlocks

#### Failure Conditions
- High error rate (> 5%)
- Database connection pool exhausted

#### Rollback Steps
Delete test issues.

---

## Automation Scripts

### Setup: Install Playwright

```bash
npm install -D @playwright/test
npx playwright install
```

### Example: [TC-001] First Visit Test

```typescript
// tests/e2e/tc001-first-visit.spec.ts
import { test, expect } from '@playwright/test';

test('[TC-001] First visit shows onboarding', async ({ page }) => {
  // Preconditions: Clear storage
  await page.goto('http://localhost:5173/clear-storage.html');
  await page.waitForTimeout(1000);

  // Test steps
  await page.goto('http://localhost:5173/');

  // Verification: URL should be onboarding
  await expect(page).toHaveURL(/.*onboarding/);

  // Verification: Welcome message visible
  await expect(page.locator('text=/Welcome to Parrot Agent/i')).toBeVisible();

  // Verification: Create button exists
  await expect(page.locator('button:has-text("Create")')).toBeVisible();
});
```

### Example: [TC-102] Create Agent Test

```typescript
// tests/e2e/tc102-create-agent.spec.ts
import { test, expect } from '@playwright/test';

test('[TC-102] Create new agent', async ({ page }) => {
  // Preconditions: Login as board user
  await page.goto('http://localhost:5173/');
  await page.click('button:has-text("Login as Board")');
  await page.waitForURL(/.*dashboard|\/$/);

  // Test steps
  await page.goto('http://localhost:5173/agents');
  await page.click('button:has-text("New Agent")');

  // Fill form
  await page.fill('input[name="name"]', 'TestAgent01');
  await page.fill('input[name="role"]', 'Software Engineer');
  await page.fill('textarea[name="description"]', 'Test agent for E2E');

  // Submit
  await page.click('button[type="submit"]:has-text("Create")');

  // Verification: Redirected to detail page
  await page.waitForURL(/.*\/agents\/[0-9a-f-]+/);

  // Verification: Agent name displayed
  await expect(page.locator('text=TestAgent01')).toBeVisible();

  // Verification: Status badge
  await expect(page.locator('text=/active|ready/i')).toBeVisible();

  // API verification
  const response = await page.request.get('http://localhost:3100/api/agents');
  const agents = await response.json();
  expect(agents.some(a => a.name === 'TestAgent01')).toBeTruthy();
});
```

---

## Test Execution Checklist

### Daily Regression (P0 Tests)

- [ ] [TC-001] First Visit
- [ ] [TC-002] Board Login
- [ ] [TC-102] Create Agent
- [ ] [TC-202] Create Issue
- [ ] [TC-203] View Issue Detail
- [ ] [TC-204] Update Issue Status
- [ ] [TC-205] Assign Issue

### Pre-Release (P0 + P1 Tests)

- [ ] All P0 tests
- [ ] [TC-003] Logout
- [ ] [TC-101] View Agent List
- [ ] [TC-103] Edit Agent
- [ ] [TC-104] Delete Agent
- [ ] [TC-201] View Issue List
- [ ] [TC-302] Approval Workflow
- [ ] [TC-401] Performance - Agent List

### Full Suite (All Tests)

- [ ] All P0 and P1 tests
- [ ] [TC-301] Multi-Agent Collaboration
- [ ] [TC-402] Load Testing

---

## Maintenance Notes

### Adding New Test Cases

1. Follow the template structure
2. Assign unique TC-XXX ID (sequential)
3. Set priority (P0/P1/P2)
4. Write clear verification criteria
5. Add automation script if P0/P1

### Updating Test Cases

- Update **Last Updated** date in header
- Increment version if major changes
- Keep old TC-IDs, append version suffix if needed (e.g., TC-102-v2)

### Reporting Issues

If a test case fails:
1. Record failure details (screenshot, logs, timestamp)
2. File bug report with TC-ID reference
3. Mark test as "Blocked" until bug fixed

---

**Document Maintainer**: Parrot Agent Team  
**Review Cycle**: Bi-weekly  
**Next Review**: 2026-08-31
