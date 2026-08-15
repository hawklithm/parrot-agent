use crate::adapter_plugin_store::{AdapterPluginRecord, AdapterPluginStore};
use crate::npm_manager::NpmManager;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransactionError {
    #[error("npm error: {0}")]
    Npm(#[from] crate::npm_manager::NpmError),
    
    #[error("rollback failed: {0}")]
    RollbackFailed(String),
    
    #[error("transaction already committed")]
    AlreadyCommitted,
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("transaction already rolled back")]
    AlreadyRolledBack,
}

pub type TransactionResult<T> = Result<T, TransactionError>;

/// 安装事务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransactionState {
    InProgress,
    Committed,
    RolledBack,
}

/// 适配器安装事务
/// 
/// 提供原子性保证：要么完全成功，要么完全回滚
/// 
/// # 事务步骤
/// 
/// 1. 创建事务
/// 2. 执行安装操作
/// 3. 记录回滚信息
/// 4. 提交或回滚
/// 
/// # 回滚策略
/// 
/// - npm 安装失败：执行 npm uninstall
/// - 本地路径加载失败：不需要清理
/// - 插件记录保存失败：删除已安装的包
pub struct AdapterInstallTransaction {
    npm_manager: NpmManager,
    state: TransactionState,
    
    /// 是否为 npm 安装（需要回滚卸载）
    is_npm_install: bool,
    
    /// 安装的包名
    package_name: Option<String>,
    
    /// 安装前插件存储的快照
    store_snapshot: Option<Vec<AdapterPluginRecord>>,
}

impl AdapterInstallTransaction {
    /// 创建新的安装事务
    pub fn new(npm_manager: NpmManager) -> Self {
        Self {
            npm_manager,
            state: TransactionState::InProgress,
            is_npm_install: false,
            package_name: None,
            store_snapshot: None,
        }
    }
    
    pub fn install_npm_package(
        &mut self,
        package_name: &str,
        version: Option<&str>,
    ) -> TransactionResult<String> {
        if self.state != TransactionState::InProgress {
            return Err(TransactionError::AlreadyCommitted);
        }
        self.ensure_in_progress()?;
        
        tracing::info!(
            package_name = %package_name,
            version = ?version,
            "Starting npm package install transaction"
        );
        
        // 执行安装
        let installed_version = self.npm_manager.install_package(package_name, version)?;
        
        // 记录回滚信息
        self.is_npm_install = true;
        self.package_name = Some(package_name.to_string());
        
        tracing::info!(
            package_name = %package_name,
            installed_version = %installed_version,
            "npm package installed successfully"
        );
        
        Ok(installed_version)
    }
    
    pub fn validate_local_path(&mut self, local_path: &PathBuf) -> TransactionResult<()> {
        if self.state != TransactionState::InProgress {
            return Err(TransactionError::AlreadyCommitted);
        }
        
        if !local_path.exists() {
            return Err(TransactionError::RollbackFailed(format!(
                "Local path does not exist: {}",
                local_path.display()
            )));
        }
        
        let pkg_json = local_path.join("package.json");
        if !pkg_json.exists() {
            return Err(TransactionError::RollbackFailed(
                "package.json not found in local path".to_string(),
            ));
        }
        
        Ok(())
    }
    
    /// 保存插件存储快照
    pub fn save_store_snapshot(&mut self, store: &dyn AdapterPluginStore) {
        self.store_snapshot = Some(store.list());
    }
    
    /// 提交事务
    pub fn commit(mut self) -> TransactionResult<()> {
        if self.state != TransactionState::InProgress {
            return Err(TransactionError::AlreadyCommitted);
        }
        
        tracing::info!(
            package_name = ?self.package_name,
            "Committing adapter install transaction"
        );
        
        self.state = TransactionState::Committed;
        Ok(())
    }
    pub fn rollback(mut self) -> TransactionResult<()> {
        if self.state != TransactionState::InProgress {
            return Err(TransactionError::AlreadyRolledBack);
        }
        
        tracing::warn!(
            package_name = ?self.package_name,
            is_npm_install = self.is_npm_install,
            "Rolling back adapter install transaction"
        );
        
        // 如果是 npm 安装，执行 uninstall
        if self.is_npm_install {
            if let Some(package_name) = &self.package_name {
                match self.npm_manager.uninstall_package(package_name) {
                    Ok(()) => {
                        tracing::info!(
                            package_name = %package_name,
                            "npm package uninstalled during rollback"
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            error = %e,
                            package_name = %package_name,
                            "Failed to uninstall package during rollback"
                        );
                        return Err(TransactionError::RollbackFailed(format!(
                            "Failed to uninstall {}: {}",
                            package_name, e
                        )));
                    }
                }
            }
        }
        
        self.state = TransactionState::RolledBack;
        Ok(())
    }
    
    /// 确保事务处于进行中状态
    fn ensure_in_progress(&self) -> TransactionResult<()> {
        match self.state {
            TransactionState::InProgress => Ok(()),
            TransactionState::Committed => Err(TransactionError::AlreadyCommitted),
            TransactionState::RolledBack => Err(TransactionError::AlreadyRolledBack),
        }
    }
}

impl Drop for AdapterInstallTransaction {
    fn drop(&mut self) {
        // 如果事务未提交且未回滚，自动回滚
        if self.state == TransactionState::InProgress {
            tracing::warn!(
                package_name = ?self.package_name,
                "Transaction dropped without commit, auto-rolling back"
            );
            
            if self.is_npm_install {
                if let Some(package_name) = &self.package_name {
                    if let Err(e) = self.npm_manager.uninstall_package(package_name) {
                        tracing::error!(
                            error = %e,
                            package_name = %package_name,
                            "Failed to auto-rollback during drop"
                        );
                    }
                }
            }
            
            self.state = TransactionState::RolledBack;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_transaction_commit() {
        let temp_dir = TempDir::new().unwrap();
        let npm_manager = NpmManager::new(temp_dir.path());
        let mut tx = AdapterInstallTransaction::new(npm_manager);
        
        assert_eq!(tx.state, TransactionState::InProgress);
        
        tx.commit().unwrap();
        // 不能再提交
        // assert!(tx.commit().is_err()); // 编译器会阻止这个
    }
    
    #[test]
    fn test_local_path_validation() {
        let temp_dir = TempDir::new().unwrap();
        let npm_manager = NpmManager::new(temp_dir.path());
        let mut tx = AdapterInstallTransaction::new(npm_manager);
        
        // 不存在的路径应该失败
        let non_existent = PathBuf::from("/nonexistent/path");
        assert!(tx.validate_local_path(&non_existent).is_err());
    }
}
