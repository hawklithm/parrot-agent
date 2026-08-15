/// Cache Service
/// 
/// 分布式缓存管理服务

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("cache miss: {0}")]
    Miss(String),
    
    #[error("serialization error: {0}")]
    Serialization(String),
    
    #[error("cache unavailable: {0}")]
    Unavailable(String),
}

pub type CacheResult<T> = Result<T, CacheError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry<T> {
    pub key: String,
    pub value: T,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl<T> CacheEntry<T> {
    pub fn new(key: String, value: T, ttl_seconds: Option<i64>) -> Self {
        let now = chrono::Utc::now();
        let expires_at = ttl_seconds.map(|ttl| now + chrono::Duration::seconds(ttl));
        
        Self {
            key,
            value,
            expires_at,
            created_at: now,
        }
    }
    
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|exp| exp < chrono::Utc::now())
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub total_entries: usize,
    pub expired_entries: usize,
    pub hit_rate: f64,
}

pub struct CacheService {
    store: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    hits: Arc<RwLock<u64>>,
    misses: Arc<RwLock<u64>>,
}

impl CacheService {
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
            hits: Arc::new(RwLock::new(0)),
            misses: Arc::new(RwLock::new(0)),
        }
    }
    
    /// 设置缓存
    pub async fn set<T: Serialize>(
        &self,
        key: String,
        value: T,
        ttl_seconds: Option<i64>,
    ) -> CacheResult<()> {
        let entry = CacheEntry::new(key.clone(), value, ttl_seconds);
        let serialized = serde_json::to_vec(&entry)
            .map_err(|e| CacheError::Serialization(e.to_string()))?;
        
        let mut store = self.store.write().await;
        store.insert(key, serialized);
        
        Ok(())
    }
    
    /// 获取缓存
    pub async fn get<T: for<'de> Deserialize<'de>>(
        &self,
        key: &str,
    ) -> CacheResult<T> {
        let store = self.store.read().await;
        
        match store.get(key) {
            Some(data) => {
                let entry: CacheEntry<T> = serde_json::from_slice(data)
                    .map_err(|e| CacheError::Serialization(e.to_string()))?;
                
                if entry.is_expired() {
                    drop(store);
                    self.delete(key).await?;
                    
                    let mut misses = self.misses.write().await;
                    *misses += 1;
                    
                    Err(CacheError::Miss(key.to_string()))
                } else {
                    let mut hits = self.hits.write().await;
                    *hits += 1;
                    
                    Ok(entry.value)
                }
            }
            None => {
                let mut misses = self.misses.write().await;
                *misses += 1;
                
                Err(CacheError::Miss(key.to_string()))
            }
        }
    }
    
    /// 删除缓存
    pub async fn delete(&self, key: &str) -> CacheResult<()> {
        let mut store = self.store.write().await;
        store.remove(key);
        Ok(())
    }
    
    /// 清空所有缓存
    pub async fn clear(&self) -> CacheResult<()> {
        let mut store = self.store.write().await;
        store.clear();
        Ok(())
    }
    
    /// 检查键是否存在
    pub async fn exists(&self, key: &str) -> bool {
        let store = self.store.read().await;
        store.contains_key(key)
    }
    
    /// 设置过期时间
    pub async fn expire(&self, key: &str, ttl_seconds: i64) -> CacheResult<()> {
        let store = self.store.read().await;
        
        match store.get(key) {
            Some(data) => {
                let mut entry: CacheEntry<serde_json::Value> = serde_json::from_slice(data)
                    .map_err(|e| CacheError::Serialization(e.to_string()))?;
                
                entry.expires_at = Some(chrono::Utc::now() + chrono::Duration::seconds(ttl_seconds));
                
                drop(store);
                
                let serialized = serde_json::to_vec(&entry)
                    .map_err(|e| CacheError::Serialization(e.to_string()))?;
                
                let mut store = self.store.write().await;
                store.insert(key.to_string(), serialized);
                
                Ok(())
            }
            None => Err(CacheError::Miss(key.to_string())),
        }
    }
    
    /// 获取缓存统计
    pub async fn get_stats(&self) -> CacheStats {
        let store = self.store.read().await;
        let hits = *self.hits.read().await;
        let misses = *self.misses.read().await;
        
        let total = hits + misses;
        let hit_rate = if total > 0 {
            (hits as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        
        // 统计过期条目
        let expired_count = store.values().filter(|data| {
            if let Ok(entry) = serde_json::from_slice::<CacheEntry<serde_json::Value>>(data) {
                entry.is_expired()
            } else {
                false
            }
        }).count();
        
        CacheStats {
            hits,
            misses,
            total_entries: store.len(),
            expired_entries: expired_count,
            hit_rate,
        }
    }
    
    /// 清理过期条目
    pub async fn cleanup_expired(&self) -> usize {
        let store = self.store.read().await;
        let mut expired_keys = Vec::new();
        
        for (key, data) in store.iter() {
            if let Ok(entry) = serde_json::from_slice::<CacheEntry<serde_json::Value>>(data) {
                if entry.is_expired() {
                    expired_keys.push(key.clone());
                }
            }
        }
        
        drop(store);
        
        let count = expired_keys.len();
        let mut store = self.store.write().await;
        for key in expired_keys {
            store.remove(&key);
        }
        
        count
    }
    
    /// 批量获取
    pub async fn mget<T: for<'de> Deserialize<'de>>(
        &self,
        keys: &[String],
    ) -> HashMap<String, CacheResult<T>> {
        let mut results = HashMap::new();
        
        for key in keys {
            let result = self.get::<T>(key).await;
            results.insert(key.clone(), result);
        }
        
        results
    }
    
    /// 批量设置
    pub async fn mset<T: Serialize>(
        &self,
        entries: HashMap<String, (T, Option<i64>)>,
    ) -> CacheResult<()> {
        for (key, (value, ttl)) in entries {
            self.set(key, value, ttl).await?;
        }
        Ok(())
    }
}

impl Default for CacheService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_cache_set_get() {
        let cache = CacheService::new();
        
        cache.set("key1".to_string(), "value1".to_string(), None).await.unwrap();
        
        let value: String = cache.get("key1").await.unwrap();
        assert_eq!(value, "value1");
    }
    
    #[tokio::test]
    async fn test_cache_miss() {
        let cache = CacheService::new();
        
        let result: CacheResult<String> = cache.get("nonexistent").await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_cache_expiration() {
        let cache = CacheService::new();
        
        cache.set("key1".to_string(), "value1".to_string(), Some(1)).await.unwrap();
        
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        
        let result: CacheResult<String> = cache.get("key1").await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_cache_delete() {
        let cache = CacheService::new();
        
        cache.set("key1".to_string(), "value1".to_string(), None).await.unwrap();
        assert!(cache.exists("key1").await);
        
        cache.delete("key1").await.unwrap();
        assert!(!cache.exists("key1").await);
    }
    
    #[tokio::test]
    async fn test_cache_stats() {
        let cache = CacheService::new();
        
        cache.set("key1".to_string(), "value1".to_string(), None).await.unwrap();
        
        let _: String = cache.get("key1").await.unwrap();
        let _: CacheResult<String> = cache.get("key2").await;
        
        let stats = cache.get_stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hit_rate, 50.0);
    }
}
