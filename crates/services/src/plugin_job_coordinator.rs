/// Plugin Job Coordinator
/// 
/// 分布式作业协调器，负责：
/// - 跨节点作业分配
/// - 负载均衡
/// - 节点健康监控

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum CoordinatorError {
    #[error("no available workers")]
    NoAvailableWorkers,
    
    #[error("worker not found: {0}")]
    WorkerNotFound(Uuid),
    
    #[error("job assignment failed: {0}")]
    AssignmentFailed(String),
}

pub type CoordinatorResult<T> = Result<T, CoordinatorError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerNode {
    pub id: Uuid,
    pub address: String,
    pub capacity: usize,
    pub current_load: usize,
    pub is_healthy: bool,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
}

impl WorkerNode {
    pub fn load_factor(&self) -> f64 {
        if self.capacity == 0 {
            1.0
        } else {
            self.current_load as f64 / self.capacity as f64
        }
    }
    
    pub fn can_accept_job(&self) -> bool {
        self.is_healthy && self.current_load < self.capacity
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobAssignment {
    pub job_id: Uuid,
    pub worker_id: Uuid,
    pub assigned_at: chrono::DateTime<chrono::Utc>,
}

/// 分布式作业协调器
pub struct PluginJobCoordinator {
    workers: Arc<RwLock<HashMap<Uuid, WorkerNode>>>,
    assignments: Arc<RwLock<HashMap<Uuid, JobAssignment>>>,
}

impl PluginJobCoordinator {
    pub fn new() -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            assignments: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// 注册工作节点
    pub async fn register_worker(&self, worker: WorkerNode) -> CoordinatorResult<()> {
        let mut workers = self.workers.write().await;
        workers.insert(worker.id, worker);
        Ok(())
    }
    
    /// 注销工作节点
    pub async fn unregister_worker(&self, worker_id: Uuid) -> CoordinatorResult<()> {
        let mut workers = self.workers.write().await;
        workers.remove(&worker_id)
            .ok_or(CoordinatorError::WorkerNotFound(worker_id))?;
        Ok(())
    }
    
    /// 更新节点心跳
    pub async fn update_heartbeat(&self, worker_id: Uuid) -> CoordinatorResult<()> {
        let mut workers = self.workers.write().await;
        let worker = workers.get_mut(&worker_id)
            .ok_or(CoordinatorError::WorkerNotFound(worker_id))?;
        
        worker.last_heartbeat = chrono::Utc::now();
        worker.is_healthy = true;
        
        Ok(())
    }
    
    /// 分配作业到最佳节点（负载均衡）
    pub async fn assign_job(&self, job_id: Uuid) -> CoordinatorResult<Uuid> {
        let workers = self.workers.read().await;
        
        // 找到负载最低的健康节点
        let best_worker = workers.values()
            .filter(|w| w.can_accept_job())
            .min_by(|a, b| {
                a.load_factor()
                    .partial_cmp(&b.load_factor())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .ok_or(CoordinatorError::NoAvailableWorkers)?;
        
        let worker_id = best_worker.id;
        
        // 记录分配
        let mut assignments = self.assignments.write().await;
        assignments.insert(job_id, JobAssignment {
            job_id,
            worker_id,
            assigned_at: chrono::Utc::now(),
        });
        
        // 更新节点负载
        drop(assignments);
        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.get_mut(&worker_id) {
            worker.current_load += 1;
        }
        
        Ok(worker_id)
    }
    
    /// 完成作业分配
    pub async fn complete_job(&self, job_id: Uuid) -> CoordinatorResult<()> {
        let mut assignments = self.assignments.write().await;
        let assignment = assignments.remove(&job_id)
            .ok_or(CoordinatorError::AssignmentFailed("job not assigned".to_string()))?;
        
        // 减少节点负载
        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.get_mut(&assignment.worker_id) {
            worker.current_load = worker.current_load.saturating_sub(1);
        }
        
        Ok(())
    }
    
    /// 获取集群状态
    pub async fn get_cluster_status(&self) -> ClusterStatus {
        let workers = self.workers.read().await;
        let assignments = self.assignments.read().await;
        
        ClusterStatus {
            total_workers: workers.len(),
            healthy_workers: workers.values().filter(|w| w.is_healthy).count(),
            total_capacity: workers.values().map(|w| w.capacity).sum(),
            total_load: workers.values().map(|w| w.current_load).sum(),
            active_jobs: assignments.len(),
        }
    }
    
    /// 检查不健康节点
    pub async fn check_unhealthy_workers(&self, timeout_secs: i64) -> Vec<Uuid> {
        let mut workers = self.workers.write().await;
        let now = chrono::Utc::now();
        let mut unhealthy = Vec::new();
        
        for (id, worker) in workers.iter_mut() {
            let elapsed = now.signed_duration_since(worker.last_heartbeat);
            if elapsed.num_seconds() > timeout_secs {
                worker.is_healthy = false;
                unhealthy.push(*id);
            }
        }
        
        unhealthy
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatus {
    pub total_workers: usize,
    pub healthy_workers: usize,
    pub total_capacity: usize,
    pub total_load: usize,
    pub active_jobs: usize,
}

impl ClusterStatus {
    pub fn load_percentage(&self) -> f64 {
        if self.total_capacity == 0 {
            0.0
        } else {
            (self.total_load as f64 / self.total_capacity as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_worker_registration() {
        let coordinator = PluginJobCoordinator::new();
        
        let worker = WorkerNode {
            id: Uuid::new_v4(),
            address: "localhost:8080".to_string(),
            capacity: 10,
            current_load: 0,
            is_healthy: true,
            last_heartbeat: chrono::Utc::now(),
        };
        
        coordinator.register_worker(worker.clone()).await.unwrap();
        
        let status = coordinator.get_cluster_status().await;
        assert_eq!(status.total_workers, 1);
        assert_eq!(status.healthy_workers, 1);
    }
    
    #[tokio::test]
    async fn test_load_balancing() {
        let coordinator = PluginJobCoordinator::new();
        
        // 注册两个节点
        let worker1 = WorkerNode {
            id: Uuid::new_v4(),
            address: "node1:8080".to_string(),
            capacity: 5,
            current_load: 2,
            is_healthy: true,
            last_heartbeat: chrono::Utc::now(),
        };
        
        let worker2 = WorkerNode {
            id: Uuid::new_v4(),
            address: "node2:8080".to_string(),
            capacity: 5,
            current_load: 4,
            is_healthy: true,
            last_heartbeat: chrono::Utc::now(),
        };
        
        coordinator.register_worker(worker1.clone()).await.unwrap();
        coordinator.register_worker(worker2.clone()).await.unwrap();
        
        // 分配作业应该选择负载较低的节点（worker1）
        let job_id = Uuid::new_v4();
        let assigned_worker = coordinator.assign_job(job_id).await.unwrap();
        
        assert_eq!(assigned_worker, worker1.id);
    }
    
    #[tokio::test]
    async fn test_unhealthy_worker_detection() {
        let coordinator = PluginJobCoordinator::new();
        
        let worker = WorkerNode {
            id: Uuid::new_v4(),
            address: "localhost:8080".to_string(),
            capacity: 10,
            current_load: 0,
            is_healthy: true,
            last_heartbeat: chrono::Utc::now() - chrono::Duration::seconds(120),
        };
        
        coordinator.register_worker(worker.clone()).await.unwrap();
        
        let unhealthy = coordinator.check_unhealthy_workers(60).await;
        assert_eq!(unhealthy.len(), 1);
        assert_eq!(unhealthy[0], worker.id);
    }
}
