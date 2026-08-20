/// Workspace Runtime Service
/// 
/// Workspace生命周期管理、进程隔离和资源清理

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceRuntimeError {
    #[error("workspace not found: {0}")]
    NotFound(Uuid),
    
    #[error("workspace already exists: {0}")]
    AlreadyExists(Uuid),
    
    #[error("process error: {0}")]
    ProcessError(String),
    
    #[error("resource error: {0}")]
    ResourceError(String),
    
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type WorkspaceRuntimeResult<T> = Result<T, WorkspaceRuntimeError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum WorkspaceStatus {
    Creating,
    Active,
    Paused,
    Terminating,
    Terminated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRuntime {
    pub id: Uuid,
    pub name: String,
    pub root_path: PathBuf,
    pub status: WorkspaceStatus,
    pub process_ids: Vec<u32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_accessed: chrono::DateTime<chrono::Utc>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl WorkspaceRuntime {
    pub fn new(name: String, root_path: PathBuf) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            root_path,
            status: WorkspaceStatus::Creating,
            process_ids: Vec::new(),
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            metadata: HashMap::new(),
        }
    }
    
    pub fn is_active(&self) -> bool {
        self.status == WorkspaceStatus::Active
    }
    
    pub fn mark_accessed(&mut self) {
        self.last_accessed = chrono::Utc::now();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub isolation_level: IsolationLevel,
    pub resource_limits: ResourceLimits,
    pub auto_cleanup: bool,
    pub idle_timeout_secs: Option<u64>,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            isolation_level: IsolationLevel::Process,
            resource_limits: ResourceLimits::default(),
            auto_cleanup: true,
            idle_timeout_secs: Some(3600), // 1 hour
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IsolationLevel {
    None,
    Process,
    Container,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub max_memory_mb: Option<u64>,
    pub max_cpu_percent: Option<u32>,
    pub max_disk_mb: Option<u64>,
    pub max_processes: Option<u32>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_mb: Some(4096),
            max_cpu_percent: Some(80),
            max_disk_mb: Some(10240),
            max_processes: Some(100),
        }
    }
}

pub struct WorkspaceRuntimeService {
    workspaces: HashMap<Uuid, WorkspaceRuntime>,
    config: WorkspaceConfig,
}

impl WorkspaceRuntimeService {
    pub fn new(config: WorkspaceConfig) -> Self {
        Self {
            workspaces: HashMap::new(),
            config,
        }
    }
    
    /// 创建workspace
    pub fn create_workspace(
        &mut self,
        name: String,
        root_path: PathBuf,
    ) -> WorkspaceRuntimeResult<Uuid> {
        // 检查路径是否已存在
        for ws in self.workspaces.values() {
            if ws.root_path == root_path {
                return Err(WorkspaceRuntimeError::AlreadyExists(ws.id));
            }
        }
        
        // 创建目录
        std::fs::create_dir_all(&root_path)?;
        
        let mut workspace = WorkspaceRuntime::new(name, root_path);
        workspace.status = WorkspaceStatus::Active;
        
        let id = workspace.id;
        self.workspaces.insert(id, workspace);
        
        Ok(id)
    }
    
    /// 获取workspace
    pub fn get_workspace(&self, id: Uuid) -> WorkspaceRuntimeResult<&WorkspaceRuntime> {
        self.workspaces.get(&id)
            .ok_or(WorkspaceRuntimeError::NotFound(id))
    }
    
    /// 获取可变workspace
    pub fn get_workspace_mut(&mut self, id: Uuid) -> WorkspaceRuntimeResult<&mut WorkspaceRuntime> {
        self.workspaces.get_mut(&id)
            .ok_or(WorkspaceRuntimeError::NotFound(id))
    }
    
    /// 暂停workspace
    pub fn pause_workspace(&mut self, id: Uuid) -> WorkspaceRuntimeResult<()> {
        let workspace = self.get_workspace_mut(id)?;
        workspace.status = WorkspaceStatus::Paused;
        
        // 暂停关联的进程
        // TODO: 实现进程暂停逻辑
        
        Ok(())
    }
    
    /// 恢复workspace
    pub fn resume_workspace(&mut self, id: Uuid) -> WorkspaceRuntimeResult<()> {
        let workspace = self.get_workspace_mut(id)?;
        workspace.status = WorkspaceStatus::Active;
        workspace.mark_accessed();
        
        Ok(())
    }
    
    /// 销毁workspace
    pub fn destroy_workspace(&mut self, id: Uuid) -> WorkspaceRuntimeResult<()> {
        // 1. 先提取配置值
        let auto_cleanup = self.config.auto_cleanup;
        
        // 2. 更新状态为 Terminating
        {
            let workspace = self.get_workspace_mut(id)?;
            workspace.status = WorkspaceStatus::Terminating;
        }
        
        // 3. 终止所有进程
        self.terminate_processes(id)?;
        
        // 4. 清理资源
        if auto_cleanup {
            self.cleanup_resources(id)?;
        }
        
        // 5. 更新为 Terminated 并移除
        if let Some(mut workspace) = self.workspaces.remove(&id) {
            workspace.status = WorkspaceStatus::Terminated;
        }
        
        Ok(())
    }
    
    /// 终止workspace的所有进程
    fn terminate_processes(&mut self, workspace_id: Uuid) -> WorkspaceRuntimeResult<()> {
        let process_ids = self.get_workspace(workspace_id)?.process_ids.clone();

        for pid in process_ids {
            #[cfg(unix)]
            {
                use std::process::Command;
                let result = Command::new("kill")
                    .arg("-TERM")
                    .arg(pid.to_string())
                    .output();
                match result {
                    Ok(output) => {
                        // A process may have exited between discovery and cleanup.
                        // Treat that race as success, but surface other command
                        // failures so the workspace cannot be reported as clean.
                        if !output.status.success()
                            && !String::from_utf8_lossy(&output.stderr).contains("No such process")
                        {
                            return Err(WorkspaceRuntimeError::ProcessError(
                                String::from_utf8_lossy(&output.stderr).trim().to_string(),
                            ));
                        }
                    }
                    Err(error) => return Err(WorkspaceRuntimeError::ProcessError(error.to_string())),
                }
            }
            #[cfg(windows)]
            {
                use std::process::Command;
                let output = Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/T", "/F"])
                    .output()
                    .map_err(|error| WorkspaceRuntimeError::ProcessError(error.to_string()))?;
                let diagnostics = format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                if !output.status.success()
                    && !diagnostics.contains("not found")
                    && !diagnostics.contains("不存在")
                {
                    return Err(WorkspaceRuntimeError::ProcessError(diagnostics.trim().to_string()));
                }
            }
        }

        Ok(())
    }
    
    /// 清理workspace资源
    fn cleanup_resources(&mut self, workspace_id: Uuid) -> WorkspaceRuntimeResult<()> {
        let workspace = self.get_workspace(workspace_id)?;
        
        // 删除workspace目录
        if workspace.root_path.exists() {
            std::fs::remove_dir_all(&workspace.root_path)?;
        }
        
        Ok(())
    }
    
    /// 注册进程到workspace
    pub fn register_process(&mut self, workspace_id: Uuid, pid: u32) -> WorkspaceRuntimeResult<()> {
        let workspace = self.get_workspace_mut(workspace_id)?;
        
        if !workspace.process_ids.contains(&pid) {
            workspace.process_ids.push(pid);
        }
        
        workspace.mark_accessed();
        Ok(())
    }
    
    /// 取消注册进程
    pub fn unregister_process(&mut self, workspace_id: Uuid, pid: u32) -> WorkspaceRuntimeResult<()> {
        let workspace = self.get_workspace_mut(workspace_id)?;
        workspace.process_ids.retain(|&p| p != pid);
        Ok(())
    }
    
    /// 检查并清理空闲workspace
    pub fn cleanup_idle_workspaces(&mut self) -> WorkspaceRuntimeResult<Vec<Uuid>> {
        let timeout_secs = match self.config.idle_timeout_secs {
            Some(s) => s,
            None => return Ok(Vec::new()),
        };
        
        let now = chrono::Utc::now();
        let mut to_destroy = Vec::new();
        
        for (id, workspace) in &self.workspaces {
            let idle_duration = now.signed_duration_since(workspace.last_accessed);
            
            if idle_duration.num_seconds() > timeout_secs as i64 {
                to_destroy.push(*id);
            }
        }
        
        for id in &to_destroy {
            let _ = self.destroy_workspace(*id);
        }
        
        Ok(to_destroy)
    }
    
    /// 列出所有workspace
    pub fn list_workspaces(&self) -> Vec<&WorkspaceRuntime> {
        self.workspaces.values().collect()
    }
    
    /// 获取workspace统计
    pub fn get_statistics(&self) -> WorkspaceStatistics {
        let total = self.workspaces.len();
        let mut by_status = HashMap::new();
        
        for workspace in self.workspaces.values() {
            *by_status.entry(workspace.status.clone()).or_insert(0) += 1;
        }
        
        WorkspaceStatistics {
            total_workspaces: total,
            by_status,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceStatistics {
    pub total_workspaces: usize,
    pub by_status: HashMap<WorkspaceStatus, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    
    #[test]
    fn test_create_workspace() {
        let mut service = WorkspaceRuntimeService::new(WorkspaceConfig::default());
        let temp_dir = std::env::temp_dir().join("test_workspace");
        
        let id = service.create_workspace("test".to_string(), temp_dir.clone()).unwrap();
        
        assert!(temp_dir.exists());
        assert_eq!(service.workspaces.len(), 1);
        
        let workspace = service.get_workspace(id).unwrap();
        assert_eq!(workspace.name, "test");
        assert_eq!(workspace.status, WorkspaceStatus::Active);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    
    #[test]
    fn test_pause_resume() {
        let mut service = WorkspaceRuntimeService::new(WorkspaceConfig::default());
        let temp_dir = std::env::temp_dir().join("test_pause");
        
        let id = service.create_workspace("test".to_string(), temp_dir.clone()).unwrap();
        
        service.pause_workspace(id).unwrap();
        assert_eq!(service.get_workspace(id).unwrap().status, WorkspaceStatus::Paused);
        
        service.resume_workspace(id).unwrap();
        assert_eq!(service.get_workspace(id).unwrap().status, WorkspaceStatus::Active);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    
    #[test]
    fn test_destroy_workspace() {
        let mut service = WorkspaceRuntimeService::new(WorkspaceConfig::default());
        let temp_dir = std::env::temp_dir().join("test_destroy");
        
        let id = service.create_workspace("test".to_string(), temp_dir.clone()).unwrap();
        
        service.destroy_workspace(id).unwrap();
        
        assert!(!temp_dir.exists());
        assert_eq!(service.workspaces.len(), 0);
    }
    
    #[test]
    fn test_process_registration() {
        let mut service = WorkspaceRuntimeService::new(WorkspaceConfig::default());
        let temp_dir = std::env::temp_dir().join("test_process");
        
        let id = service.create_workspace("test".to_string(), temp_dir.clone()).unwrap();
        
        service.register_process(id, 1234).unwrap();
        assert_eq!(service.get_workspace(id).unwrap().process_ids.len(), 1);
        
        service.unregister_process(id, 1234).unwrap();
        assert_eq!(service.get_workspace(id).unwrap().process_ids.len(), 0);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
