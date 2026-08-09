use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// 外部适配器插件记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterPluginRecord {
    /// npm 包名或本地路径标识
    pub package_name: String,
    /// 本地路径（如果是本地安装）
    pub local_path: Option<String>,
    /// 版本号
    pub version: Option<String>,
    /// 适配器类型（对应 AdapterType）
    #[serde(rename = "type")]
    pub adapter_type: String,
    /// 安装时间（ISO 8601）
    pub installed_at: String,
}

/// 适配器插件存储 trait
pub trait AdapterPluginStore: Send + Sync {
    /// 列出所有插件记录
    fn list(&self) -> Vec<AdapterPluginRecord>;
    
    /// 根据适配器类型获取记录
    fn get(&self, adapter_type: &str) -> Option<AdapterPluginRecord>;
    
    /// 添加或更新插件记录
    fn add(&mut self, record: AdapterPluginRecord);
    
    /// 删除插件记录
    fn remove(&mut self, adapter_type: &str) -> bool;
    
    /// 检查是否存在
    fn contains(&self, adapter_type: &str) -> bool;
}

/// JSON 文件存储实现（对齐 Paperclip 的 adapter-plugins.json）
pub struct JsonFileAdapterPluginStore {
    file_path: PathBuf,
    records: Arc<RwLock<HashMap<String, AdapterPluginRecord>>>,
}

impl JsonFileAdapterPluginStore {
    /// 创建新的 JSON 文件存储
    pub fn new(file_path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let file_path = file_path.as_ref().to_path_buf();
        let records = Self::load_from_file(&file_path)?;
        
        Ok(Self {
            file_path,
            records: Arc::new(RwLock::new(records)),
        })
    }
    
    /// 从文件加载记录
    fn load_from_file(file_path: &Path) -> Result<HashMap<String, AdapterPluginRecord>, std::io::Error> {
        if !file_path.exists() {
            // 文件不存在，返回空 map
            return Ok(HashMap::new());
        }
        
        let content = fs::read_to_string(file_path)?;
        let records: Vec<AdapterPluginRecord> = serde_json::from_str(&content)
            .unwrap_or_else(|_| Vec::new());
        
        Ok(records
            .into_iter()
            .map(|r| (r.adapter_type.clone(), r))
            .collect())
    }
    
    /// 保存记录到文件
    fn save_to_file(&self) -> Result<(), std::io::Error> {
        let records = self.records.read();
        let records_vec: Vec<&AdapterPluginRecord> = records.values().collect();
        let content = serde_json::to_string_pretty(&records_vec)?;
        drop(records); // 显式释放读锁
        
        // 确保父目录存在
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        fs::write(&self.file_path, content)?;
        Ok(())
    }
}

impl AdapterPluginStore for JsonFileAdapterPluginStore {
    fn list(&self) -> Vec<AdapterPluginRecord> {
        let records = self.records.read();
        records.values().cloned().collect()
    }
    
    fn get(&self, adapter_type: &str) -> Option<AdapterPluginRecord> {
        let records = self.records.read();
        records.get(adapter_type).cloned()
    }
    
    fn add(&mut self, record: AdapterPluginRecord) {
        let adapter_type = record.adapter_type.clone();
        {
            let mut records = self.records.write();
            records.insert(adapter_type, record);
        }
        
        // 持久化到文件
        if let Err(e) = self.save_to_file() {
            eprintln!("Failed to save adapter plugin record: {}", e);
        }
    }
    
    fn remove(&mut self, adapter_type: &str) -> bool {
        let removed = {
            let mut records = self.records.write();
            records.remove(adapter_type).is_some()
        };
        
        if removed {
            // 持久化到文件
            if let Err(e) = self.save_to_file() {
                eprintln!("Failed to save adapter plugin record after removal: {}", e);
            }
        }
        
        removed
    }
    
    fn contains(&self, adapter_type: &str) -> bool {
        let records = self.records.read();
        records.contains_key(adapter_type)
    }
}

/// 内存存储实现（用于测试）
pub struct InMemoryAdapterPluginStore {
    records: HashMap<String, AdapterPluginRecord>,
}

impl InMemoryAdapterPluginStore {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }
}

impl Default for InMemoryAdapterPluginStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AdapterPluginStore for InMemoryAdapterPluginStore {
    fn list(&self) -> Vec<AdapterPluginRecord> {
        self.records.values().cloned().collect()
    }
    
    fn get(&self, adapter_type: &str) -> Option<AdapterPluginRecord> {
        self.records.get(adapter_type).cloned()
    }
    
    fn add(&mut self, record: AdapterPluginRecord) {
        self.records.insert(record.adapter_type.clone(), record);
    }
    
    fn remove(&mut self, adapter_type: &str) -> bool {
        self.records.remove(adapter_type).is_some()
    }
    
    fn contains(&self, adapter_type: &str) -> bool {
        self.records.contains_key(adapter_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_in_memory_store() {
        let mut store = InMemoryAdapterPluginStore::new();
        
        let record = AdapterPluginRecord {
            package_name: "test-adapter".to_string(),
            local_path: None,
            version: Some("1.0.0".to_string()),
            adapter_type: "test".to_string(),
            installed_at: "2024-01-01T00:00:00Z".to_string(),
        };
        
        // 添加
        store.add(record.clone());
        assert!(store.contains("test"));
        
        // 获取
        let retrieved = store.get("test").unwrap();
        assert_eq!(retrieved.package_name, "test-adapter");
        
        // 列出
        let all = store.list();
        assert_eq!(all.len(), 1);
        
        // 删除
        assert!(store.remove("test"));
        assert!(!store.contains("test"));
    }
}
