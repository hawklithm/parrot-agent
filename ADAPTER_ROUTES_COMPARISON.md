# Paperclip vs Parrot-Agent Adapters 路由对比分析

## 📊 概览

| 项目 | 路由数量 | 全局路由 | Company-scoped 路由 |
|------|----------|----------|---------------------|
| **Paperclip** | 10 | 10 | 0 |
| **Parrot-Agent** | 17 | 10 | 7 |

### ✅ 全局路由对齐率: 100%

所有 Paperclip 的全局路由端点在 Parrot-Agent 中都有对应实现。

---

## 🔍 详细实现对比

### 1️⃣ GET /adapters - 列出所有适配器

#### Paperclip 实现 ✅
```typescript
// 返回字段
{
  type: string,
  label: string,
  source: "builtin" | "external",
  modelsCount: number,
  loaded: boolean,
  disabled: boolean,
  capabilities: {...},
  acp?: string,
  overriddenBuiltin?: boolean,
  overridePaused?: boolean,
  version?: string,
  packageName?: string,
  isLocalPath?: boolean
}

// 功能
- 区分 built-in 和 external 适配器
- 包含禁用状态
- 包含覆盖状态
- 包含版本信息
- 按 type 字母排序
- 权限: assertBoardOrgAccess
```

#### Parrot-Agent 实现 ❌
```rust
// 返回字段
{
  "adapterType": string,
  "label": string,
  "supportsInstructionsBundle": boolean
}

// 问题
❌ 缺少 source (builtin/external) 区分
❌ 缺少 modelsCount, loaded, disabled 状态
❌ 缺少覆盖管理相关字段
❌ 缺少版本和包信息
❌ 无排序
❌ 无权限检查
```

---

### 2️⃣ POST /adapters/install - 安装外部适配器

#### Paperclip 实现 ✅
```typescript
// 请求体
{
  packageName: string,      // npm 包名或本地路径
  isLocalPath?: boolean,    // 是否为本地路径
  version?: string          // 版本号
}

// 功能
1. npm install 包到 plugins 目录
2. 支持本地路径安装
3. 版本后缀解析 (@scope/name@1.2.3)
4. 检测重新安装
5. 加载并注册适配器
6. 持久化到 adapter-plugins.json
7. 返回 requiresRestart 标志

// 返回
{
  type: string,
  packageName: string,
  version: string,
  installedAt: string,
  requiresRestart: boolean
}

// 权限: assertInstanceAdmin
```

#### Parrot-Agent 实现 ❌
```rust
// 当前实现: 空 stub
Ok((
    StatusCode::CREATED,
    Json(serde_json::json!({
        "adapterType": adapter_type,
        "installed": true,
        "message": "..."
    }))
))

// 问题
❌ 空实现，仅返回 mock 数据
❌ 缺少 npm 安装逻辑
❌ 缺少外部适配器加载
❌ 缺少持久化存储
❌ 缺少重新安装检测
❌ 无权限检查
```

---

### 3️⃣ PATCH /adapters/:type - 启用/禁用适配器

#### Paperclip 实现 ✅
```typescript
// 请求体
{
  disabled: boolean
}

// 功能
- 调用 setAdapterDisabled(type, disabled)
- 验证适配器是否存在
- 返回是否实际变化

// 返回
{
  type: string,
  disabled: boolean,
  changed: boolean
}

// 权限: assertInstanceAdmin
```

#### Parrot-Agent 实现 ❌
```rust
// 当前实现: 语义不符
// Paperclip: 启用/禁用适配器
// Parrot:    更新适配器配置（任意 JSON）

Ok(Json(serde_json::json!({
    "adapterType": adapter_type_str,
    "config": payload,  // 直接返回输入
    "updated": true
})))

// 问题
❌ 语义完全不同
❌ 缺少 disabled 状态管理
❌ 缺少实际功能
❌ 无验证
❌ 无权限检查
```

---

### 4️⃣ PATCH /adapters/:type/override - 暂停/恢复覆盖

#### Paperclip 实现 ✅
```typescript
// 请求体
{
  paused: boolean
}

// 功能
- 暂停/恢复外部适配器对 builtin 类型的覆盖
- 仅适用于 builtin 类型
- 调用 setOverridePaused(type, paused)
- 已运行的 session 保持原适配器

// 返回
{
  type: string,
  paused: boolean,
  changed: boolean
}

// 验证: 检查是否为 BUILTIN_ADAPTER_TYPES
// 权限: assertInstanceAdmin
```

#### Parrot-Agent 实现 ❌
```rust
// 当前实现: 空 stub
Ok(Json(serde_json::json!({
    "adapterType": adapter_type_str,
    "override": payload,
    "applied": true
})))

// 问题
❌ 空实现
❌ 缺少 builtin 覆盖管理逻辑
❌ 无验证
❌ 无权限检查
```

---

### 5️⃣ DELETE /adapters/:type - 卸载适配器

#### Paperclip 实现 ✅
```typescript
// 功能
1. 检查是否为 external 适配器
2. 保护 builtin 适配器不被删除
3. npm uninstall（如果是 npm 包）
4. unregisterServerAdapter(type)
5. removeAdapterPlugin(type)

// 返回
{
  type: string,
  removed: true
}

// 权限: assertInstanceAdmin
```

#### Parrot-Agent 实现 ❌
```rust
// 当前实现
Ok(StatusCode::NO_CONTENT)

// 问题
❌ 空实现
❌ 缺少卸载逻辑
❌ 无验证
❌ 无权限检查
```

---

### 6️⃣ POST /adapters/:type/reload - 重新加载适配器

#### Paperclip 实现 ✅
```typescript
// 功能
1. 清除 ESM 模块缓存
2. 重新导入适配器模块
3. 重新注册到运行时
4. 更新版本信息
5. 清除 config schema 缓存

// 返回
{
  type: string,
  version: string,
  reloaded: true
}

// 验证: 仅允许 external 适配器
// 权限: assertInstanceAdmin
```

#### Parrot-Agent 实现 ❌
```rust
// 当前实现: 空 stub
Ok(Json(serde_json::json!({
    "adapterType": adapter_type_str,
    "reloaded": true
})))

// 问题
❌ 空实现
❌ 缺少重新加载逻辑
❌ 无验证
❌ 无权限检查
```

---

### 7️⃣ POST /adapters/:type/reinstall - 重新安装适配器

#### Paperclip 实现 ✅
```typescript
// 功能
1. 仅适用于 npm 包（不支持 local path）
2. npm install 最新版本
3. 重新加载适配器
4. 更新版本信息

// 返回
{
  type: string,
  version: string,
  reinstalled: true
}

// 验证: 检查是否为 external + 非 local path
// 权限: assertInstanceAdmin
```

#### Parrot-Agent 实现 ❌
```rust
// 当前实现: 空 stub
Ok(Json(serde_json::json!({
    "adapterType": adapter_type_str,
    "reinstalled": true
})))

// 问题
❌ 空实现
❌ 缺少重新安装逻辑
❌ 无验证
❌ 无权限检查
```

---

### 8️⃣ GET /adapters/:type/config-schema - 配置 Schema

#### Paperclip 实现 ✅
```typescript
// 功能
1. 调用 adapter.getConfigSchema()
2. 30 秒 TTL 缓存
3. 返回动态生成的 schema

// 返回
AdapterConfigSchema {
  sections: [...],
  fields: [...]
}

// 验证: 检查适配器是否支持 getConfigSchema()
// 权限: assertBoardOrgAccess
```

#### Parrot-Agent 实现 ⚠️
```rust
// 当前实现: 部分实现
Ok(Json(serde_json::json!({
    "schema": {
        "type": "object",
        "properties": {}
    }
})))

// 问题
⚠️ 返回 mock schema，未调用实际 adapter
❌ 缺少缓存
❌ 无验证
❌ 无权限检查
```

---

### 9️⃣ GET /adapters/:type/ui-parser.js - UI 解析器

#### Paperclip 实现 ✅
```typescript
// 功能
1. 从适配器包的 package.json "./ui-parser" 入口加载
2. 返回自包含的 ESM 模块
3. 用于自定义 run-log 解析

// 返回
Content-Type: application/javascript
<ui-parser source code>

// 权限: assertBoardOrgAccess
```

#### Parrot-Agent 实现 ❌
```rust
// 当前实现: 空 stub
Ok(Response::builder()
    .status(StatusCode::OK)
    .header("content-type", "application/javascript")
    .body("// UI Parser placeholder".into())?)

// 问题
❌ 返回占位符
❌ 缺少实际 UI parser 加载
❌ 无验证
❌ 无权限检查
```

---

## 🏢 Parrot-Agent 独有的 Company-scoped 路由（C 系列）

这些路由是 Parrot-Agent 的架构扩展，Paperclip 中不存在：

1. `GET /companies/:company_id/adapters` - 列出公司可用适配器
2. `GET /companies/:company_id/adapters/:adapter_type` - 获取适配器详情
3. `GET /companies/:company_id/adapters/:adapter_type/models` - 列出模型
4. `POST /companies/:company_id/adapters/:adapter_type/detect-model` - 检测模型
5. `GET /companies/:company_id/adapters/:adapter_type/detect-model` - 检测模型（GET）
6. `GET /companies/:company_id/adapters/:adapter_type/model-profiles` - 模型配置
7. `POST /companies/:company_id/adapters/:adapter_type/test-environment` - 测试环境

**说明**: 这些路由是合理的多租户扩展，无需对齐 Paperclip。

---

## 📋 需要实现的功能清单

### 🚨 高优先级（核心功能）

1. **外部适配器管理系统**
   - [ ] AdapterPluginRecord 持久化存储
   - [ ] 外部适配器加载器
   - [ ] npm 包管理集成
   - [ ] 本地路径适配器支持

2. **Builtin/External 区分**
   - [ ] BUILTIN_ADAPTER_TYPES 常量
   - [ ] 注册表区分内置和外部适配器
   - [ ] 覆盖管理（override paused）

3. **状态管理**
   - [ ] 启用/禁用状态
   - [ ] getDisabledAdapterTypes()
   - [ ] setAdapterDisabled()

4. **GET /adapters 完整实现**
   - [ ] 返回所有必需字段
   - [ ] 合并 builtin + external 数据
   - [ ] 按 type 排序

5. **POST /adapters/install 完整实现**
   - [ ] npm install 逻辑
   - [ ] 本地路径支持
   - [ ] 版本解析
   - [ ] 重新安装检测
   - [ ] 持久化

### ⚠️ 中优先级（运维功能）

6. **PATCH /adapters/:type 完整实现**
   - [ ] 修正语义（启用/禁用，而非配置更新）
   - [ ] disabled 状态管理

7. **DELETE /adapters/:type 完整实现**
   - [ ] npm uninstall
   - [ ] 注册表清理
   - [ ] 持久化清理

8. **POST /adapters/:type/reload 完整实现**
   - [ ] 模块缓存清除
   - [ ] 重新加载逻辑

9. **POST /adapters/:type/reinstall 完整实现**
   - [ ] npm 重新安装
   - [ ] 版本更新

### 📝 低优先级（增强功能）

10. **PATCH /adapters/:type/override**
    - [ ] 覆盖暂停/恢复
    - [ ] Builtin 保护

11. **GET /adapters/:type/config-schema**
    - [ ] 调用实际 adapter.getConfigSchema()
    - [ ] 30 秒 TTL 缓存

12. **GET /adapters/:type/ui-parser.js**
    - [ ] 从适配器包加载 UI parser
    - [ ] package.json 入口解析

13. **权限系统**
    - [ ] assertInstanceAdmin 实现
    - [ ] assertBoardOrgAccess 实现

---

## 🎯 实现策略建议

### 阶段 1: 核心基础设施（1-2 天）
1. 设计 AdapterPluginRecord 持久化（JSON 文件或数据库）
2. 实现 BUILTIN_ADAPTER_TYPES 常量
3. 扩展 AdapterRegistry 支持 builtin/external 区分
4. 实现 disabled 状态管理

### 阶段 2: 外部适配器管理（2-3 天）
1. 实现 POST /adapters/install（npm + local path）
2. 实现 DELETE /adapters/:type
3. 实现 POST /adapters/:type/reload
4. 实现 POST /adapters/:type/reinstall

### 阶段 3: 状态和配置（1-2 天）
1. 完整实现 GET /adapters
2. 修正 PATCH /adapters/:type 语义
3. 实现 PATCH /adapters/:type/override
4. 实现 GET /adapters/:type/config-schema（带缓存）

### 阶段 4: UI 和权限（1 天）
1. 实现 GET /adapters/:type/ui-parser.js
2. 实现权限检查中间件

---

## 🔧 技术实现建议

### Rust 端需要的新模块

```rust
// crates/services/src/adapter_plugin_store.rs
pub struct AdapterPluginRecord {
    pub package_name: String,
    pub local_path: Option<String>,
    pub version: Option<String>,
    pub adapter_type: String,
    pub installed_at: String,
}

pub trait AdapterPluginStore {
    fn list(&self) -> Vec<AdapterPluginRecord>;
    fn get(&self, adapter_type: &str) -> Option<AdapterPluginRecord>;
    fn add(&mut self, record: AdapterPluginRecord);
    fn remove(&mut self, adapter_type: &str);
}

// crates/services/src/adapter_loader.rs
pub async fn load_external_adapter(
    package_name: &str,
    local_path: Option<&str>
) -> Result<Arc<dyn ServerAdapterModule>, AdapterError>;

// crates/services/src/npm_executor.rs
pub async fn npm_install(
    package: &str,
    version: Option<&str>,
    plugins_dir: &Path
) -> Result<String, NpmError>;

pub async fn npm_uninstall(
    package: &str,
    plugins_dir: &Path
) -> Result<(), NpmError>;
```

---

## 📊 总结

| 指标 | 状态 |
|------|------|
| **路由端点对齐** | ✅ 100% (10/10) |
| **实现完整度** | ❌ ~10% (大部分为空 stub) |
| **Builtin/External 管理** | ❌ 缺失 |
| **持久化存储** | ❌ 缺失 |
| **npm 集成** | ❌ 缺失 |
| **权限系统** | ❌ 缺失 |

**结论**: Parrot-Agent 已完成路由骨架，但核心功能实现几乎为空，需要完整实现外部适配器管理系统才能与 Paperclip 对齐。
