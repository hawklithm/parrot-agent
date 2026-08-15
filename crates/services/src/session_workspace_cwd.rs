/// Session Workspace CWD
/// 
/// Session工作目录管理、路径解析和目录切换

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum CwdError {
    #[error("session not found: {0}")]
    SessionNotFound(Uuid),
    
    #[error("invalid path: {0}")]
    InvalidPath(String),
    
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    
    #[error("path not found: {0}")]
    PathNotFound(String),
}

pub type CwdResult<T> = Result<T, CwdError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionWorkspace {
    pub session_id: Uuid,
    pub workspace_root: PathBuf,
    pub current_dir: PathBuf,
    pub allowed_paths: Vec<PathBuf>,
}

impl SessionWorkspace {
    pub fn new(session_id: Uuid, workspace_root: PathBuf) -> Self {
        let current_dir = workspace_root.clone();
        
        Self {
            session_id,
            workspace_root: workspace_root.clone(),
            current_dir,
            allowed_paths: vec![workspace_root],
        }
    }
    
    /// 检查路径是否在允许范围内
    pub fn is_path_allowed(&self, path: &Path) -> bool {
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.current_dir.join(path)
        };
        
        for allowed in &self.allowed_paths {
            if abs_path.starts_with(allowed) {
                return true;
            }
        }
        
        false
    }
    
    /// 解析相对路径为绝对路径
    pub fn resolve_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.current_dir.join(path)
        }
    }
}

pub struct SessionWorkspaceCwdService {
    sessions: HashMap<Uuid, SessionWorkspace>,
}

impl SessionWorkspaceCwdService {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }
    
    /// 创建session workspace
    pub fn create_session(
        &mut self,
        session_id: Uuid,
        workspace_root: PathBuf,
    ) -> CwdResult<()> {
        let session_ws = SessionWorkspace::new(session_id, workspace_root);
        self.sessions.insert(session_id, session_ws);
        Ok(())
    }
    
    /// 删除session workspace
    pub fn remove_session(&mut self, session_id: Uuid) -> CwdResult<()> {
        self.sessions.remove(&session_id);
        Ok(())
    }
    
    /// 获取session workspace
    pub fn get_session(&self, session_id: Uuid) -> CwdResult<&SessionWorkspace> {
        self.sessions.get(&session_id)
            .ok_or(CwdError::SessionNotFound(session_id))
    }
    
    /// 获取可变session workspace
    pub fn get_session_mut(&mut self, session_id: Uuid) -> CwdResult<&mut SessionWorkspace> {
        self.sessions.get_mut(&session_id)
            .ok_or(CwdError::SessionNotFound(session_id))
    }
    
    /// 获取当前工作目录
    pub fn get_cwd(&self, session_id: Uuid) -> CwdResult<PathBuf> {
        let session = self.get_session(session_id)?;
        Ok(session.current_dir.clone())
    }
    
    /// 切换工作目录
    pub fn change_dir(&mut self, session_id: Uuid, path: &Path) -> CwdResult<()> {
        let session = self.get_session_mut(session_id)?;
        
        // 解析路径
        let target_path = session.resolve_path(path);
        
        // 检查路径是否存在
        if !target_path.exists() {
            return Err(CwdError::PathNotFound(target_path.display().to_string()));
        }
        
        // 检查是否是目录
        if !target_path.is_dir() {
            return Err(CwdError::InvalidPath(format!("{} is not a directory", target_path.display())));
        }
        
        // 检查权限
        if !session.is_path_allowed(&target_path) {
            return Err(CwdError::PermissionDenied(target_path.display().to_string()));
        }
        
        // 更新当前目录
        session.current_dir = target_path;
        
        Ok(())
    }
    
    /// 解析路径
    pub fn resolve_path(&self, session_id: Uuid, path: &Path) -> CwdResult<PathBuf> {
        let session = self.get_session(session_id)?;
        Ok(session.resolve_path(path))
    }
    
    /// 检查路径访问权限
    pub fn check_access(&self, session_id: Uuid, path: &Path) -> CwdResult<bool> {
        let session = self.get_session(session_id)?;
        let resolved = session.resolve_path(path);
        Ok(session.is_path_allowed(&resolved))
    }
    
    /// 添加允许的路径
    pub fn add_allowed_path(&mut self, session_id: Uuid, path: PathBuf) -> CwdResult<()> {
        let session = self.get_session_mut(session_id)?;
        
        if !session.allowed_paths.contains(&path) {
            session.allowed_paths.push(path);
        }
        
        Ok(())
    }
    
    /// 移除允许的路径
    pub fn remove_allowed_path(&mut self, session_id: Uuid, path: &Path) -> CwdResult<()> {
        let session = self.get_session_mut(session_id)?;
        session.allowed_paths.retain(|p| p != path);
        Ok(())
    }
    
    /// 获取相对路径
    pub fn get_relative_path(&self, session_id: Uuid, path: &Path) -> CwdResult<PathBuf> {
        let session = self.get_session(session_id)?;
        let resolved = session.resolve_path(path);
        
        match resolved.strip_prefix(&session.current_dir) {
            Ok(rel) => Ok(rel.to_path_buf()),
            Err(_) => Ok(resolved),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    
    #[test]
    fn test_create_session() {
        let mut service = SessionWorkspaceCwdService::new();
        let session_id = Uuid::new_v4();
        let root = env::temp_dir();
        
        service.create_session(session_id, root.clone()).unwrap();
        
        let cwd = service.get_cwd(session_id).unwrap();
        assert_eq!(cwd, root);
    }
    
    #[test]
    fn test_resolve_path() {
        let mut service = SessionWorkspaceCwdService::new();
        let session_id = Uuid::new_v4();
        let root = env::temp_dir();
        
        service.create_session(session_id, root.clone()).unwrap();
        
        let relative = Path::new("subdir");
        let resolved = service.resolve_path(session_id, relative).unwrap();
        
        assert_eq!(resolved, root.join("subdir"));
    }
    
    #[test]
    fn test_path_permission() {
        let mut service = SessionWorkspaceCwdService::new();
        let session_id = Uuid::new_v4();
        let root = env::temp_dir();
        
        service.create_session(session_id, root.clone()).unwrap();
        
        // 在允许范围内的路径
        assert!(service.check_access(session_id, &root.join("file.txt")).unwrap());
        
        // 在允许范围外的路径
        let outside = PathBuf::from("/etc/passwd");
        assert!(!service.check_access(session_id, &outside).unwrap());
    }
    
    #[test]
    fn test_change_dir() {
        let mut service = SessionWorkspaceCwdService::new();
        let session_id = Uuid::new_v4();
        let root = env::temp_dir();
        
        service.create_session(session_id, root.clone()).unwrap();
        
        // 切换到存在的目录
        let result = service.change_dir(session_id, &root);
        assert!(result.is_ok());
        
        // 切换到不存在的目录
        let nonexistent = root.join("nonexistent_dir_12345");
        let result = service.change_dir(session_id, &nonexistent);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_add_allowed_path() {
        let mut service = SessionWorkspaceCwdService::new();
        let session_id = Uuid::new_v4();
        let root = env::temp_dir();
        
        service.create_session(session_id, root.clone()).unwrap();
        
        let new_path = PathBuf::from("/tmp/new_allowed");
        service.add_allowed_path(session_id, new_path.clone()).unwrap();
        
        assert!(service.check_access(session_id, &new_path.join("file.txt")).unwrap());
    }
}
