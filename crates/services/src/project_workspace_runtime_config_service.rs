use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 项目workspace运行时配置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectWorkspaceRuntimeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_runtime: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desired_state: Option<WorkspaceDesiredState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_states: Option<HashMap<String, ServiceState>>,
}

/// Workspace期望状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceDesiredState {
    Running,
    Stopped,
    Manual,
}

/// 服务状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServiceState {
    Running,
    Stopped,
    Manual,
}

fn is_record(value: &serde_json::Value) -> bool {
    value.is_object()
}

fn clone_record(value: &serde_json::Value) -> Option<HashMap<String, serde_json::Value>> {
    if let Some(obj) = value.as_object() {
        Some(obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    } else {
        None
    }
}

fn read_desired_state(value: &serde_json::Value) -> Option<WorkspaceDesiredState> {
    value.as_str().and_then(|s| match s {
        "running" => Some(WorkspaceDesiredState::Running),
        "stopped" => Some(WorkspaceDesiredState::Stopped),
        "manual" => Some(WorkspaceDesiredState::Manual),
        _ => None,
    })
}

fn read_service_states(value: &serde_json::Value) -> Option<HashMap<String, ServiceState>> {
    if !is_record(value) {
        return None;
    }

    let entries: HashMap<String, ServiceState> = value
        .as_object()?
        .iter()
        .filter_map(|(key, val)| {
            val.as_str().and_then(|s| match s {
                "running" => Some((key.clone(), ServiceState::Running)),
                "stopped" => Some((key.clone(), ServiceState::Stopped)),
                "manual" => Some((key.clone(), ServiceState::Manual)),
                _ => None,
            })
        })
        .collect();

    if entries.is_empty() {
        None
    } else {
        Some(entries)
    }
}

/// 从metadata中读取项目workspace运行时配置
pub fn read_project_workspace_runtime_config(
    metadata: Option<&HashMap<String, serde_json::Value>>,
) -> Option<ProjectWorkspaceRuntimeConfig> {
    let metadata = metadata?;
    let runtime_config = metadata.get("runtimeConfig")?;

    if !is_record(runtime_config) {
        return None;
    }

    let obj = runtime_config.as_object()?;

    let config = ProjectWorkspaceRuntimeConfig {
        workspace_runtime: obj
            .get("workspaceRuntime")
            .and_then(|v| clone_record(v)),
        desired_state: obj.get("desiredState").and_then(|v| read_desired_state(v)),
        service_states: obj.get("serviceStates").and_then(|v| read_service_states(v)),
    };

    let has_config = config.workspace_runtime.is_some()
        || config.desired_state.is_some()
        || config.service_states.is_some();

    if has_config {
        Some(config)
    } else {
        None
    }
}

/// 合并项目workspace运行时配置到metadata
pub fn merge_project_workspace_runtime_config(
    metadata: Option<&HashMap<String, serde_json::Value>>,
    patch: Option<ProjectWorkspaceRuntimeConfig>,
) -> Option<HashMap<String, serde_json::Value>> {
    let mut next_metadata = metadata.cloned().unwrap_or_default();

    let current = read_project_workspace_runtime_config(Some(&next_metadata)).unwrap_or(
        ProjectWorkspaceRuntimeConfig {
            workspace_runtime: None,
            desired_state: None,
            service_states: None,
        },
    );

    // 如果patch是None，删除runtimeConfig
    let patch = match patch {
        Some(p) => p,
        None => {
            next_metadata.remove("runtimeConfig");
            return if next_metadata.is_empty() {
                None
            } else {
                Some(next_metadata)
            };
        }
    };

    let next_config = ProjectWorkspaceRuntimeConfig {
        workspace_runtime: patch.workspace_runtime.or(current.workspace_runtime),
        desired_state: patch.desired_state.or(current.desired_state),
        service_states: patch.service_states.or(current.service_states),
    };

    // 如果所有字段都是None，删除runtimeConfig
    if next_config.workspace_runtime.is_none()
        && next_config.desired_state.is_none()
        && next_config.service_states.is_none()
    {
        next_metadata.remove("runtimeConfig");
    } else {
        // 序列化配置到metadata
        if let Ok(config_value) = serde_json::to_value(&next_config) {
            next_metadata.insert("runtimeConfig".to_string(), config_value);
        }
    }

    if next_metadata.is_empty() {
        None
    } else {
        Some(next_metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_empty_config() {
        let metadata: HashMap<String, serde_json::Value> = HashMap::new();
        let config = read_project_workspace_runtime_config(Some(&metadata));
        assert!(config.is_none());
    }

    #[test]
    fn test_read_desired_state() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "runtimeConfig".to_string(),
            serde_json::json!({
                "desiredState": "running"
            }),
        );

        let config = read_project_workspace_runtime_config(Some(&metadata)).unwrap();
        assert_eq!(config.desired_state, Some(WorkspaceDesiredState::Running));
    }

    #[test]
    fn test_merge_config() {
        let metadata = None;
        let patch = ProjectWorkspaceRuntimeConfig {
            workspace_runtime: None,
            desired_state: Some(WorkspaceDesiredState::Running),
            service_states: None,
        };

        let result = merge_project_workspace_runtime_config(metadata, Some(patch));
        assert!(result.is_some());

        let result_metadata = result.unwrap();
        let config = read_project_workspace_runtime_config(Some(&result_metadata)).unwrap();
        assert_eq!(config.desired_state, Some(WorkspaceDesiredState::Running));
    }

    #[test]
    fn test_merge_removes_config_when_patch_is_none() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "runtimeConfig".to_string(),
            serde_json::json!({
                "desiredState": "running"
            }),
        );

        let result = merge_project_workspace_runtime_config(Some(&metadata), None);
        
        // 如果只有runtimeConfig，删除后应该返回None
        assert!(result.is_none() || !result.unwrap().contains_key("runtimeConfig"));
    }
}
