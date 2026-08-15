/// Dashboard Service
/// 
/// 实时统计数据、趋势分析和数据聚合

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum DashboardError {
    #[error("metric not found: {0}")]
    MetricNotFound(String),
    
    #[error("invalid time range")]
    InvalidTimeRange,
}

pub type DashboardResult<T> = Result<T, DashboardError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    pub name: String,
    pub value: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub tags: HashMap<String, String>,
}

impl Metric {
    pub fn new(name: String, value: f64) -> Self {
        Self {
            name,
            value,
            timestamp: chrono::Utc::now(),
            tags: HashMap::new(),
        }
    }
    
    pub fn with_tag(mut self, key: String, value: String) -> Self {
        self.tags.insert(key, value);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesData {
    pub metric_name: String,
    pub data_points: Vec<DataPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPoint {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendAnalysis {
    pub metric_name: String,
    pub direction: TrendDirection,
    pub change_rate: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrendDirection {
    Up,
    Down,
    Stable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardWidget {
    pub id: Uuid,
    pub title: String,
    pub widget_type: WidgetType,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WidgetType {
    Counter,
    LineChart,
    BarChart,
    PieChart,
    Table,
    Text,
}

pub struct DashboardService {
    metrics: HashMap<String, Vec<Metric>>,
    widgets: HashMap<Uuid, DashboardWidget>,
}

impl DashboardService {
    pub fn new() -> Self {
        Self {
            metrics: HashMap::new(),
            widgets: HashMap::new(),
        }
    }
    
    /// 记录指标
    pub fn record_metric(&mut self, metric: Metric) {
        self.metrics.entry(metric.name.clone())
            .or_insert_with(Vec::new)
            .push(metric);
    }
    
    /// 获取最新指标值
    pub fn get_latest_metric(&self, metric_name: &str) -> DashboardResult<f64> {
        let metrics = self.metrics.get(metric_name)
            .ok_or_else(|| DashboardError::MetricNotFound(metric_name.to_string()))?;
        
        metrics.last()
            .map(|m| m.value)
            .ok_or_else(|| DashboardError::MetricNotFound(metric_name.to_string()))
    }
    
    /// 获取时间序列数据
    pub fn get_time_series(
        &self,
        metric_name: &str,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> DashboardResult<TimeSeriesData> {
        if start >= end {
            return Err(DashboardError::InvalidTimeRange);
        }
        
        let metrics = self.metrics.get(metric_name)
            .ok_or_else(|| DashboardError::MetricNotFound(metric_name.to_string()))?;
        
        let data_points: Vec<DataPoint> = metrics.iter()
            .filter(|m| m.timestamp >= start && m.timestamp <= end)
            .map(|m| DataPoint {
                timestamp: m.timestamp,
                value: m.value,
            })
            .collect();
        
        Ok(TimeSeriesData {
            metric_name: metric_name.to_string(),
            data_points,
        })
    }
    
    /// 聚合统计
    pub fn aggregate(
        &self,
        metric_name: &str,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> DashboardResult<AggregateStats> {
        let time_series = self.get_time_series(metric_name, start, end)?;
        
        if time_series.data_points.is_empty() {
            return Ok(AggregateStats {
                count: 0,
                sum: 0.0,
                avg: 0.0,
                min: 0.0,
                max: 0.0,
                stddev: 0.0,
            });
        }
        
        let values: Vec<f64> = time_series.data_points.iter().map(|p| p.value).collect();
        
        let count = values.len();
        let sum: f64 = values.iter().sum();
        let avg = sum / count as f64;
        
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        
        let variance: f64 = values.iter()
            .map(|v| (v - avg).powi(2))
            .sum::<f64>() / count as f64;
        let stddev = variance.sqrt();
        
        Ok(AggregateStats {
            count,
            sum,
            avg,
            min,
            max,
            stddev,
        })
    }
    
    /// 趋势分析
    pub fn analyze_trend(
        &self,
        metric_name: &str,
        window_hours: i64,
    ) -> DashboardResult<TrendAnalysis> {
        let end = chrono::Utc::now();
        let start = end - chrono::Duration::hours(window_hours);
        
        let time_series = self.get_time_series(metric_name, start, end)?;
        
        if time_series.data_points.len() < 2 {
            return Ok(TrendAnalysis {
                metric_name: metric_name.to_string(),
                direction: TrendDirection::Stable,
                change_rate: 0.0,
                confidence: 0.0,
            });
        }
        
        // 简单线性回归
        let n = time_series.data_points.len() as f64;
        let x_values: Vec<f64> = (0..time_series.data_points.len())
            .map(|i| i as f64)
            .collect();
        let y_values: Vec<f64> = time_series.data_points.iter()
            .map(|p| p.value)
            .collect();
        
        let x_mean = x_values.iter().sum::<f64>() / n;
        let y_mean = y_values.iter().sum::<f64>() / n;
        
        let numerator: f64 = x_values.iter().zip(y_values.iter())
            .map(|(x, y)| (x - x_mean) * (y - y_mean))
            .sum();
        
        let denominator: f64 = x_values.iter()
            .map(|x| (x - x_mean).powi(2))
            .sum();
        
        let slope = if denominator != 0.0 {
            numerator / denominator
        } else {
            0.0
        };
        
        let direction = if slope > 0.1 {
            TrendDirection::Up
        } else if slope < -0.1 {
            TrendDirection::Down
        } else {
            TrendDirection::Stable
        };
        
        let change_rate = slope;
        let confidence = (numerator.powi(2) / (denominator * denominator)).min(1.0);
        
        Ok(TrendAnalysis {
            metric_name: metric_name.to_string(),
            direction,
            change_rate,
            confidence,
        })
    }
    
    /// 添加widget
    pub fn add_widget(&mut self, widget: DashboardWidget) -> Uuid {
        let id = widget.id;
        self.widgets.insert(id, widget);
        id
    }
    
    /// 获取widget
    pub fn get_widget(&self, id: Uuid) -> Option<&DashboardWidget> {
        self.widgets.get(&id)
    }
    
    /// 移除widget
    pub fn remove_widget(&mut self, id: Uuid) -> Option<DashboardWidget> {
        self.widgets.remove(&id)
    }
    
    /// 获取所有widget
    pub fn list_widgets(&self) -> Vec<&DashboardWidget> {
        self.widgets.values().collect()
    }
    
    /// 清理旧数据
    pub fn cleanup_old_metrics(&mut self, retention_days: i64) {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days);
        
        for metrics in self.metrics.values_mut() {
            metrics.retain(|m| m.timestamp > cutoff);
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AggregateStats {
    pub count: usize,
    pub sum: f64,
    pub avg: f64,
    pub min: f64,
    pub max: f64,
    pub stddev: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_record_and_get_metric() {
        let mut service = DashboardService::new();
        
        let metric = Metric::new("cpu_usage".to_string(), 75.5);
        service.record_metric(metric);
        
        let value = service.get_latest_metric("cpu_usage").unwrap();
        assert_eq!(value, 75.5);
    }
    
    #[test]
    fn test_time_series() {
        let mut service = DashboardService::new();
        
        let now = chrono::Utc::now();
        
        for i in 0..5 {
            let mut metric = Metric::new("requests".to_string(), (i * 10) as f64);
            metric.timestamp = now + chrono::Duration::minutes(i);
            service.record_metric(metric);
        }
        
        let start = now - chrono::Duration::minutes(1);
        let end = now + chrono::Duration::minutes(10);
        
        let time_series = service.get_time_series("requests", start, end).unwrap();
        assert_eq!(time_series.data_points.len(), 5);
    }
    
    #[test]
    fn test_aggregate() {
        let mut service = DashboardService::new();
        
        let now = chrono::Utc::now();
        
        for value in [10.0, 20.0, 30.0, 40.0, 50.0] {
            let mut metric = Metric::new("test".to_string(), value);
            metric.timestamp = now;
            service.record_metric(metric);
        }
        
        let stats = service.aggregate(
            "test",
            now - chrono::Duration::hours(1),
            now + chrono::Duration::hours(1),
        ).unwrap();
        
        assert_eq!(stats.count, 5);
        assert_eq!(stats.avg, 30.0);
        assert_eq!(stats.min, 10.0);
        assert_eq!(stats.max, 50.0);
    }
    
    #[test]
    fn test_trend_analysis() {
        let mut service = DashboardService::new();
        
        let now = chrono::Utc::now();
        
        // 上升趋势
        for i in 0..10 {
            let mut metric = Metric::new("growth".to_string(), (i * 10) as f64);
            metric.timestamp = now + chrono::Duration::hours(i);
            service.record_metric(metric);
        }
        
        let trend = service.analyze_trend("growth", 24).unwrap();
        assert_eq!(trend.direction, TrendDirection::Up);
        assert!(trend.change_rate > 0.0);
    }
    
    #[test]
    fn test_widget_management() {
        let mut service = DashboardService::new();
        
        let widget = DashboardWidget {
            id: Uuid::new_v4(),
            title: "CPU Usage".to_string(),
            widget_type: WidgetType::LineChart,
            config: serde_json::json!({}),
        };
        
        let id = service.add_widget(widget.clone());
        
        assert!(service.get_widget(id).is_some());
        
        service.remove_widget(id);
        assert!(service.get_widget(id).is_none());
    }
}
