use chrono::{DateTime, Timelike, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// 工具运行时审计写入失败指标名称
pub const TOOL_RUNTIME_AUDIT_WRITE_FAILURE_METRIC: &str = "audit_write_failed";

/// 将时间戳归入分钟bucket
fn minute_bucket(date: DateTime<Utc>) -> DateTime<Utc> {
    date.with_second(0)
        .unwrap()
        .with_nanosecond(0)
        .unwrap()
}

/// 增加工具运行时指标计数器
pub async fn increment_tool_runtime_metric_counter(
    pool: &PgPool,
    company_id: Uuid,
    metric: &str,
    at: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    let at = at.unwrap_or_else(Utc::now);
    let bucket_start_at = minute_bucket(at);
    let query = r#"
        INSERT INTO tool_runtime_metric_counters 
            (company_id, metric, bucket_start_at, count, created_at, updated_at)
        VALUES ($1, $2, $3, 1, $4, $5)
        ON CONFLICT (company_id, metric, bucket_start_at)
        DO UPDATE SET
            count = tool_runtime_metric_counters.count + 1,
            updated_at = EXCLUDED.updated_at
    "#;

    sqlx::query(query)
        .bind(company_id)
        .bind(metric)
        .bind(bucket_start_at)
        .bind(at)
        .bind(at)
        .execute(pool)
        .await?;

    Ok(())
}

/// 记录工具运行时审计写入失败
pub async fn record_tool_runtime_audit_write_failure(pool: &PgPool, company_id: Uuid) {
    if let Err(error) = increment_tool_runtime_metric_counter(
        pool,
        company_id,
        TOOL_RUNTIME_AUDIT_WRITE_FAILURE_METRIC,
        None,
    )
    .await
    {
        eprintln!(
            "[tool-runtime-metrics] Failed to record audit write failure counter: company_id={}, error={}",
            company_id, error
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_minute_bucket() {
        let date = Utc.with_ymd_and_hms(2026, 8, 16, 14, 35, 47).unwrap();
        let bucket = minute_bucket(date);
        
        assert_eq!(bucket.year(), 2026);
        assert_eq!(bucket.month(), 8);
        assert_eq!(bucket.day(), 16);
        assert_eq!(bucket.hour(), 14);
        assert_eq!(bucket.minute(), 35);
        assert_eq!(bucket.second(), 0);
        assert_eq!(bucket.nanosecond(), 0);
    }

    #[test]
    fn test_minute_bucket_already_on_minute() {
        let date = Utc.with_ymd_and_hms(2026, 8, 16, 14, 35, 0).unwrap();
        let bucket = minute_bucket(date);
        
        assert_eq!(date, bucket);
    }
}
