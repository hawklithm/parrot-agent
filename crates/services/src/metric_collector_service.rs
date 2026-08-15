/// Metric Collector Service
/// 
/// 指标收集、聚合和查询

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
    Summary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    pub name: String,
    pub metric_type: MetricType,
    pub value: f64,
    pub tags: HashMap<String, String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Metric {
    pub fn counter(name: String, value: f64) -> Self {
        Self {
            name,
            metric_type: MetricType::Counter,
            value,
            tags: HashMap::new(),
            timestamp: chrono::Utc::now(),
        }
    }
    
    pub fn gauge(name: String, value: f64) -> Self {
        Self {
            name,
            metric_type: MetricType::Gauge,
            value,
            tags: HashMap::new(),
            timestamp: chrono::Utc::now(),
        }
    }
    
    pub fn with_tag(mut self, key: String, value: String) -> Self {
        self.tags.insert(key, value);
        self
    }
    
    pub fn with_tags(mut self, tags: HashMap<String, String>) -> Self {
        self.tags.extend(tags);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSummary {
    pub name: String,
    pub count: usize,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
}

pub struct MetricCollectorService {
    metrics: Arc<RwLock<Vec<Metric>>>,
}

impl MetricCollectorService {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// 记录指标
    pub async fn record(&self, metric: Metric) {
        let mut metrics = self.metrics.write().await;
        metrics.push(metric);
    }
    
    /// 记录计数器
    pub async fn record_counter(&self, name: String, value: f64, tags: Option<HashMap<String, String>>) {
        let mut metric = Metric::counter(name, value);
        if let Some(tags) = tags {
            metric = metric.with_tags(tags);
        }
        self.record(metric).await;
    }
    
    /// 记录仪表盘
    pub async fn record_gauge(&self, name: String, value: f64, tags: Option<HashMap<String, String>>) {
        let mut metric = Metric::gauge(name, value);
        if let Some(tags) = tags {
            metric = metric.with_tags(tags);
        }
        self.record(metric).await;
    }
    
    /// 增加计数器
    pub async fn increment(&self, name: String, tags: Option<HashMap<String, String>>) {
        self.record_counter(name, 1.0, tags).await;
    }
    
    /// 查询指标
    pub async fn query(
        &self,
        name: Option<&str>,
        start_time: Option<chrono::DateTime<chrono::Utc>>,
        end_time: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Vec<Metric> {
        let metrics = self.metrics.read().await;
        
        metrics.iter()
            .filter(|m| {
                if let Some(n) = name {
                    if m.name != n {
                        return false;
                    }
                }
                
                if let Some(start) = start_time {
                    if m.timestamp < start {
                        return false;
                    }
                }
                
                if let Some(end) = end_time {
                    if m.timestamp > end {
                        return false;
                    }
                }
                
                true
            })
            .cloned()
            .collect()
    }
    
    /// 获取指标摘要
    pub async fn get_summary(&self, name: &str) -> Option<MetricSummary> {
        let metrics = self.query(Some(name), None, None).await;
        
        if metrics.is_empty() {
            return None;
        }
        
        let mut values: Vec<f64> = metrics.iter().map(|m| m.value).collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let count = values.len();
        let sum: f64 = values.iter().sum();
        let min = *values.first().unwrap();
        let max = *values.last().unwrap();
        let avg = sum / count as f64;
        
        let p50 = percentile(&values, 50.0);
        let p95 = percentile(&values, 95.0);
        let p99 = percentile(&values, 99.0);
        
        Some(MetricSummary {
            name: name.to_string(),
            count,
            sum,
            min,
            max,
            avg,
            p50,
            p95,
            p99,
        })
    }
    
    /// 获取所有指标名称
    pub async fn get_metric_names(&self) -> Vec<String> {
        let metrics = self.metrics.read().await;
        let mut names: Vec<String> = metrics.iter()
            .map(|m| m.name.clone())
            .collect();
        
        names.sort();
        names.dedup();
        names
    }
    
    /// 按标签查询
    pub async fn query_by_tags(&self, tags: HashMap<String, String>) -> Vec<Metric> {
        let metrics = self.metrics.read().await;
        
        metrics.iter()
            .filter(|m| {
                tags.iter().all(|(k, v)| {
                    m.tags.get(k).map(|mv| mv == v).unwrap_or(false)
                })
            })
            .cloned()
            .collect()
    }
    
    /// 聚合计数器
    pub async fn aggregate_counter(&self, name: &str) -> f64 {
        let metrics = self.query(Some(name), None, None).await;
        
        metrics.iter()
            .filter(|m| m.metric_type == MetricType::Counter)
            .map(|m| m.value)
            .sum()
    }
    
    /// 获取最新仪表盘值
    pub async fn get_latest_gauge(&self, name: &str) -> Option<f64> {
        let metrics = self.query(Some(name), None, None).await;
        
        metrics.iter()
            .filter(|m| m.metric_type == MetricType::Gauge)
            .max_by_key(|m| m.timestamp)
            .map(|m| m.value)
    }
    
    /// 清理旧指标
    pub async fn cleanup_old(&self, days: i64) -> usize {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days);
        let mut metrics = self.metrics.write().await;
        
        let original_len = metrics.len();
        metrics.retain(|m| m.timestamp > cutoff);
        let removed = original_len - metrics.len();
        
        removed
    }
    
    /// 获取指标数量
    pub async fn count(&self) -> usize {
        let metrics = self.metrics.read().await;
        metrics.len()
    }
    
    /// 清空所有指标
    pub async fn clear(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.clear();
    }
}

fn percentile(sorted_values: &[f64], percentile: f64) -> f64 {
    if sorted_values.is_empty() {
        return 0.0;
    }
    
    let index = (percentile / 100.0 * (sorted_values.len() - 1) as f64).round() as usize;
    sorted_values[index.min(sorted_values.len() - 1)]
}

impl Default for MetricCollectorService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_record_counter() {
        let service = MetricCollectorService::new();
        
        service.record_counter("requests".to_string(), 1.0, None).await;
        service.record_counter("requests".to_string(), 1.0, None).await;
        
        let total = service.aggregate_counter("requests").await;
        assert_eq!(total, 2.0);
    }
    
    #[tokio::test]
    async fn test_record_gauge() {
        let service = MetricCollectorService::new();
        
        service.record_gauge("cpu_usage".to_string(), 50.0, None).await;
        service.record_gauge("cpu_usage".to_string(), 75.0, None).await;
        
        let latest = service.get_latest_gauge("cpu_usage").await.unwrap();
        assert_eq!(latest, 75.0);
    }
    
    #[tokio::test]
    async fn test_query_metrics() {
        let service = MetricCollectorService::new();
        
        service.record_counter("api_calls".to_string(), 1.0, None).await;
        service.record_counter("api_calls".to_string(), 1.0, None).await;
        service.record_gauge("memory".to_string(), 100.0, None).await;
        
        let api_metrics = service.query(Some("api_calls"), None, None).await;
        assert_eq!(api_metrics.len(), 2);
        
        let memory_metrics = service.query(Some("memory"), None, None).await;
        assert_eq!(memory_metrics.len(), 1);
    }
    
    #[tokio::test]
    async fn test_metric_summary() {
        let service = MetricCollectorService::new();
        
        for value in [10.0, 20.0, 30.0, 40.0, 50.0] {
            service.record_counter("response_time".to_string(), value, None).await;
        }
        
        let summary = service.get_summary("response_time").await.unwrap();
        assert_eq!(summary.count, 5);
        assert_eq!(summary.min, 10.0);
        assert_eq!(summary.max, 50.0);
        assert_eq!(summary.avg, 30.0);
    }
    
    #[tokio::test]
    async fn test_query_by_tags() {
        let service = MetricCollectorService::new();
        
        let mut tags1 = HashMap::new();
        tags1.insert("env".to_string(), "prod".to_string());
        
        let mut tags2 = HashMap::new();
        tags2.insert("env".to_string(), "dev".to_string());
        
        service.record_counter("requests".to_string(), 1.0, Some(tags1.clone())).await;
        service.record_counter("requests".to_string(), 1.0, Some(tags2)).await;
        
        let prod_metrics = service.query_by_tags(tags1).await;
        assert_eq!(prod_metrics.len(), 1);
    }
}
