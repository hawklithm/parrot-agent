# Database Reset Tools Documentation

**Version**: 1.0  
**Last Updated**: 2026-08-17

---

## Overview

本文档说明如何重置 Parrot Agent 数据库到干净的测试状态。

---

## Tools

### 1. Shell Script (推荐用于 E2E 测试)

**文件**: `scripts/reset_database.sh`

**用法**:
```bash
cd parrot-agent
./scripts/reset_database.sh
```

**功能**:
- 连接到数据库
- 删除所有表 (DROP SCHEMA CASCADE)
- 重新运行所有 migrations
- 插入测试种子数据

**输出示例**:
```
========================================
Parrot Agent Database Reset
========================================
Database: parrot_agent_dev
[OK] Dropping schema...
[OK] Running migrations...
Applied 19 migrations
[OK] Inserting seed data...
[OK] Database reset complete!
========================================
```

---

### 2. Rust Binary (用于开发)

**文件**: `crates/server/src/bin/reset_database.rs`

**用法**:
```bash
cd parrot-agent
cargo run --bin reset_database
```

**功能**:
- 读取 `scripts/reset_database.sql`
- 执行 SQL 脚本
- 验证结果

**环境变量**:
```bash
DATABASE_URL=postgres://postgres:admin123@localhost:5432/parrot_agent_dev
```

---

## SQL Script Structure

**文件**: `scripts/reset_database.sql`

```sql
-- 1. 清理所有表
DROP SCHEMA public CASCADE;
CREATE SCHEMA public;

-- 2. 运行 migrations (由工具自动执行)

-- 3. 插入种子数据

-- 3.1 创建默认公司
INSERT INTO companies (
  id, 
  name,
  issue_prefix,
  require_board_approval_for_new_agents,
  created_at,
  updated_at
) VALUES (
  '00000000-0000-0000-0000-000000000000',
  'Default Company',
  'CMP',
  false,
  NOW(),
  NOW()
) ON CONFLICT (id) DO NOTHING;

-- 3.2 创建 Board 用户
INSERT INTO auth_users (
  id,
  email,
  name,
  created_at,
  updated_at
) VALUES (
  '48592512-465a-4ed7-9b12-ca554ee636e8',
  'board@local.dev',
  'Local Board User',
  NOW(),
  NOW()
) ON CONFLICT (id) DO NOTHING;

-- 3.3 将 Board 用户添加到公司
INSERT INTO company_memberships (
  company_id,
  principal_type,
  principal_id,
  membership_role,
  status,
  created_at
) VALUES (
  '00000000-0000-0000-0000-000000000000',
  'auth_user',
  '48592512-465a-4ed7-9b12-ca554ee636e8',
  'admin',
  'active',
  NOW()
) ON CONFLICT (company_id, principal_type, principal_id) DO NOTHING;
```

---

## Seed Data Reference

### Default Company

| Field | Value |
|-------|-------|
| `id` | `00000000-0000-0000-0000-000000000000` |
| `name` | `Default Company` |
| `issue_prefix` | `CMP` |
| `require_board_approval_for_new_agents` | `false` |

### Board User

| Field | Value |
|-------|-------|
| `id` | `48592512-465a-4ed7-9b12-ca554ee636e8` |
| `email` | `board@local.dev` |
| `name` | `Local Board User` |

---

## Troubleshooting

### Error: "relation ... does not exist"

**原因**: Migrations 没有完全运行。

**解决方案**:
```bash
cd parrot-agent
sqlx migrate run
```

### Error: "column ... does not exist"

**原因**: SQL 脚本中的字段名与实际 schema 不匹配。

**解决方案**:
1. 检查最新的 migration 文件
2. 更新 `reset_database.sql` 中的字段名
3. 参考 `00_init_schema_unified.sql`

### Error: "null value in column ... violates not-null constraint"

**原因**: 缺少必填字段。

**解决方案**:
在 INSERT 语句中添加缺失的字段。

---

## CI/CD Integration

### GitHub Actions Example

```yaml
name: E2E Tests
on: [push, pull_request]

jobs:
  e2e:
    runs-on: ust
    services:
      postgres:
        image: postgres:15
        env:
          POSTGRES_PASSWORD: admin123
          POSTGRES_DB: parrot_agent_dev
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5

    steps:
      - uses: actions/checkout@v3
      
      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Install sqlx-cli
        run: cargo install sqlx-cli --no-default-features --features postgres

      - name: Reset Database
        working-directory: ./parrot-agent
        run: |
          sqlx migrate run
          cargo run --bin reset_database

      - name: Run E2E Tests
        run: npm run test:e2e
```

---

## Best Practices

1. **始终在测试前重置数据库**:避免测试间的数据污染
2. **使用固定的 UUID**:便于测试断言
3. **保持种子数据最小化**:只包含必需的默认数据
4. **版本控制 SQL 脚本**:确保团队同步

---

**Maintainer**: Parrot Agent Team
