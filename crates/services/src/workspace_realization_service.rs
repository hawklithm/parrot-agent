/// Workspace Realization Service
/// 
/// Workspace实例化、环境准备和依赖安装

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum RealizationError {
    #[error("realization failed: {0}")]
    Failed(String),
    
    #[error("workspace not found: {0}")]
    WorkspaceNotFound(Uuid),
    
    #[error("dependency error: {0}")]
    DependencyError(String),
    
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type RealizationResult<T> = Result<T, RealizationError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceTemplate {
    pub template_id: String,
    pub name: String,
    pub description: String,
    pub base_image: Option<String>,
    pub dependencies: Vec<Dependency>,
    pub environment: HashMap<String, String>,
    pub init_scripts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version: Option<String>,
    pub package_manager: PackageManager,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PackageManager {
    Npm,
    Yarn,
    Pnpm,
    Pip,
    Cargo,
    Apt,
    Brew,
}

impl PackageManager {
    pub fn install_command(&self) -> &str {
        match self {
            PackageManager::Npm => "npm install",
            PackageManager::Yarn => "yarn add",
            PackageManager::Pnpm => "pnpm add",
            PackageManager::Pip => "pip install",
            PackageManager::Cargo => "cargo add",
            PackageManager::Apt => "apt-get install -y",
            PackageManager::Brew => "brew install",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealizationPlan {
    pub workspace_id: Uuid,
    pub template: WorkspaceTemplate,
    pub steps: Vec<RealizationStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RealizationStep {
    CreateDirectory(PathBuf),
    InstallDependency(Dependency),
    SetEnvironment(String, String),
    RunScript(String),
    CopyFiles { source: PathBuf, dest: PathBuf },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RealizationStatus {
    Pending,
    InProgress,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealizationProgress {
    pub workspace_id: Uuid,
    pub status: RealizationStatus,
    pub current_step: usize,
    pub total_steps: usize,
    pub logs: Vec<String>,
}

pub struct WorkspaceRealizationService {
    realizations: HashMap<Uuid, RealizationProgress>,
    templates: HashMap<String, WorkspaceTemplate>,
}

impl WorkspaceRealizationService {
    pub fn new() -> Self {
        Self {
            realizations: HashMap::new(),
            templates: HashMap::new(),
        }
    }
    
    /// 注册template
    pub fn register_template(&mut self, template: WorkspaceTemplate) {
        self.templates.insert(template.template_id.clone(), template);
    }
    
    /// 获取template
    pub fn get_template(&self, template_id: &str) -> Option<&WorkspaceTemplate> {
        self.templates.get(template_id)
    }
    
    /// 创建实例化计划
    pub fn create_plan(
        &self,
        workspace_id: Uuid,
        template_id: &str,
        workspace_root: &PathBuf,
    ) -> RealizationResult<RealizationPlan> {
        let template = self.get_template(template_id)
            .ok_or_else(|| RealizationError::Failed(format!("template {} not found", template_id)))?
            .clone();
        
        let mut steps = Vec::new();
        
        // 创建目录
        steps.push(RealizationStep::CreateDirectory(workspace_root.clone()));
        
        // 安装依赖
        for dep in &template.dependencies {
            steps.push(RealizationStep::InstallDependency(dep.clone()));
        }
        
        // 设置环境变量
        for (key, value) in &template.environment {
            steps.push(RealizationStep::SetEnvironment(key.clone(), value.clone()));
        }
        
        // 运行初始化脚本
        for script in &template.init_scripts {
            steps.push(RealizationStep::RunScript(script.clone()));
        }
        
        Ok(RealizationPlan {
            workspace_id,
            template,
            steps,
        })
    }
    
    /// 执行实例化
    pub fn realize_workspace(&mut self, plan: RealizationPlan) -> RealizationResult<()> {
        let workspace_id = plan.workspace_id;
        let total_steps = plan.steps.len();
        
        // 初始化进度
        let mut progress = RealizationProgress {
            workspace_id,
            status: RealizationStatus::InProgress,
            current_step: 0,
            total_steps,
            logs: Vec::new(),
        };
        
        self.realizations.insert(workspace_id, progress.clone());
        
        // 执行每个步骤
        for (i, step) in plan.steps.iter().enumerate() {
            progress.current_step = i + 1;
            
            let step_result = self.execute_step(step, &mut progress);
            
            if let Err(e) = step_result {
                progress.status = RealizationStatus::Failed(e.to_string());
                self.realizations.insert(workspace_id, progress);
                return Err(e);
            }
            
            self.realizations.insert(workspace_id, progress.clone());
        }
        
        // 完成
        progress.status = RealizationStatus::Completed;
        self.realizations.insert(workspace_id, progress);
        
        Ok(())
    }
    
    /// 执行单个步骤
    fn execute_step(
        &self,
        step: &RealizationStep,
        progress: &mut RealizationProgress,
    ) -> RealizationResult<()> {
        match step {
            RealizationStep::CreateDirectory(path) => {
                progress.logs.push(format!("Creating directory: {}", path.display()));
                std::fs::create_dir_all(path)?;
            }
            
            RealizationStep::InstallDependency(dep) => {
                progress.logs.push(format!("Installing dependency: {}", dep.name));
                
                // 简化实现 - 实际应该调用包管理器
                let cmd = dep.package_manager.install_command();
                let package = if let Some(version) = &dep.version {
                    format!("{}@{}", dep.name, version)
                } else {
                    dep.name.clone()
                };
                
                progress.logs.push(format!("Command: {} {}", cmd, package));
                
                // TODO: 实际执行安装命令
            }
            
            RealizationStep::SetEnvironment(key, value) => {
                progress.logs.push(format!("Setting environment: {}={}", key, value));
                std::env::set_var(key, value);
            }
            
            RealizationStep::RunScript(script) => {
                progress.logs.push(format!("Running script: {}", script));
                
                // TODO: 实际执行脚本
            }
            
            RealizationStep::CopyFiles { source, dest } => {
                progress.logs.push(format!("Copying {} to {}", source.display(), dest.display()));
                
                if source.is_dir() {
                    copy_dir_all(source, dest)?;
                } else {
                    std::fs::copy(source, dest)?;
                }
            }
        }
        
        Ok(())
    }
    
    /// 获取实例化进度
    pub fn get_progress(&self, workspace_id: Uuid) -> Option<&RealizationProgress> {
        self.realizations.get(&workspace_id)
    }
    
    /// 列出所有template
    pub fn list_templates(&self) -> Vec<&WorkspaceTemplate> {
        self.templates.values().collect()
    }
}

/// 递归复制目录
fn copy_dir_all(src: &PathBuf, dst: &PathBuf) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_register_template() {
        let mut service = WorkspaceRealizationService::new();
        
        let template = WorkspaceTemplate {
            template_id: "nodejs".to_string(),
            name: "Node.js Workspace".to_string(),
            description: "A workspace for Node.js projects".to_string(),
            base_image: None,
            dependencies: vec![
                Dependency {
                    name: "typescript".to_string(),
                    version: Some("5.0.0".to_string()),
                    package_manager: PackageManager::Npm,
                }
            ],
            environment: HashMap::new(),
            init_scripts: vec!["npm init -y".to_string()],
        };
        
        service.register_template(template);
        
        assert!(service.get_template("nodejs").is_some());
    }
    
    #[test]
    fn test_create_plan() {
        let mut service = WorkspaceRealizationService::new();
        
        let template = WorkspaceTemplate {
            template_id: "test".to_string(),
            name: "Test".to_string(),
            description: "".to_string(),
            base_image: None,
            dependencies: vec![],
            environment: HashMap::new(),
            init_scripts: vec![],
        };
        
        service.register_template(template);
        
        let workspace_id = Uuid::new_v4();
        let root = PathBuf::from("/tmp/test_workspace");
        
        let plan = service.create_plan(workspace_id, "test", &root).unwrap();
        
        assert_eq!(plan.workspace_id, workspace_id);
        assert!(plan.steps.len() > 0);
    }
}
