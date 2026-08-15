/// Environment Probe Service
/// 
/// 环境探测和检测

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum EnvironmentProbeError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("probe failed: {0}")]
    ProbeFailed(String),
}

pub type EnvironmentProbeResult<T> = Result<T, EnvironmentProbeError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentInfo {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub os_type: String,
    pub os_version: String,
    pub architecture: String,
    pub available_memory_mb: i64,
    pub available_disk_gb: i64,
    pub cpu_cores: i32,
    pub probed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub success: bool,
    pub info: Option<EnvironmentInfo>,
    pub errors: Vec<String>,
}

pub struct EnvironmentProbeService {
    pool: PgPool,
}

impl EnvironmentProbeService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn probe_environment(
        &self,
        environment_id: Uuid,
    ) -> EnvironmentProbeResult<ProbeResult> {
        let mut errors = Vec::new();
        
        // 探测OS信息
        let os_info = self.detect_os().await;
        if let Err(e) = &os_info {
            errors.push(format!("OS detection failed: {}", e));
        }
        
        // 探测资源信息
        let resources = self.detect_resources().await;
        if let Err(e) = &resources {
            errors.push(format!("Resource detection failed: {}", e));
        }
        
        if errors.is_empty() {
            let (os_type, os_version, arch) = os_info.unwrap();
            let (memory, disk, cpu) = resources.unwrap();
            
            let id = Uuid::new_v4();
            
            let _result: uuid::Uuid = sqlx::query_scalar(
                r#"
                INSERT INTO environment_probes 
                (id, environment_id, os_type, os_version, architecture, 
                 available_memory_mb, available_disk_gb, cpu_cores, probed_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                RETURNING id
                "#
            )
            .bind(id)
            .bind(environment_id)
            .bind(&os_type)
            .bind(&os_version)
            .bind(&arch)
            .bind(memory)
            .bind(disk)
            .bind(cpu)
            .bind(chrono::Utc::now())
            .fetch_one(&self.pool)
            .await?;
            
            Ok(ProbeResult {
                success: true,
                info: Some(EnvironmentInfo {
                    id,
                    environment_id,
                    os_type,
                    os_version,
                    architecture: arch,
                    available_memory_mb: memory,
                    available_disk_gb: disk,
                    cpu_cores: cpu,
                    probed_at: chrono::Utc::now(),
                }),
                errors: vec![],
            })
        } else {
            Ok(ProbeResult {
                success: false,
                info: None,
                errors,
            })
        }
    }
    
    pub async fn get_latest_probe(
        &self,
        environment_id: Uuid,
    ) -> EnvironmentProbeResult<Option<EnvironmentInfo>> {
        let row = sqlx::query(
            r#"
            SELECT id, environment_id, os_type, os_version, architecture,
                   available_memory_mb, available_disk_gb, cpu_cores, probed_at
            FROM environment_probes
            WHERE environment_id = $1
            ORDER BY probed_at DESC
            LIMIT 1
            "#
        )
        .bind(environment_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.map(|r| EnvironmentInfo {
            id: r.get("id"),
            environment_id: r.get("environment_id"),
            os_type: r.get("os_type"),
            os_version: r.get("os_version"),
            architecture: r.get("architecture"),
            available_memory_mb: r.get("available_memory_mb"),
            available_disk_gb: r.get("available_disk_gb"),
            cpu_cores: r.get("cpu_cores"),
            probed_at: r.get("probed_at"),
        }))
    }
    
    async fn detect_os(&self) -> Result<(String, String, String), String> {
        // 简化实现：返回模拟数据
        Ok((
            "Linux".to_string(),
            "6.1.0".to_string(),
            "x86_64".to_string(),
        ))
    }
    
    async fn detect_resources(&self) -> Result<(i64, i64, i32), String> {
        // 简化实现：返回模拟数据
        Ok((16384, 500, 8))
    }
}
