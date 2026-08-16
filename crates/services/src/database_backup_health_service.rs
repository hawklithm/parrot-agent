use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// 数据库备份健康警告代码
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseBackupHealthWarningCode {
    DatabaseBackupCheckFailed,
    DatabaseBackupLastFailure,
    DatabaseBackupMissing,
    DatabaseBackupStale,
}

/// 数据库备份健康警告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseBackupHealthWarning {
    pub code: DatabaseBackupHealthWarningCode,
    pub message: String,
}

/// 数据库备份健康状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseBackupHealthStatus {
    pub enabled: bool,
    pub status: BackupStatus,
    pub backup_dir: String,
    pub max_age_hours: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_backup: Option<LatestBackupInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_failure: Option<LastFailureInfo>,
    pub warnings: Vec<DatabaseBackupHealthWarning>,
}

/// 备份状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BackupStatus {
    Ok,
    Warning,
}

/// 最新备份信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatestBackupInfo {
    pub path: String,
    pub age_hours: f64,
    pub size_bytes: u64,
}

/// 最后失败信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastFailureInfo {
    pub age_hours: f64,
    pub message: String,
}

/// 检查数据库备份健康选项
#[derive(Debug, Clone)]
pub struct InspectDatabaseBackupHealthOptions {
    pub enabled: bool,
    pub backup_dir: String,
    pub max_age_hours: f64,
    pub alert_file: Option<String>,
    pub alert_files: Option<Vec<String>>,
    pub now: Option<DateTime<Utc>>,
}

fn round_hours(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn alert_file_candidates(opts: &InspectDatabaseBackupHealthOptions) -> Vec<String> {
    let mut candidates = Vec::new();

    if let Some(ref files) = opts.alert_files {
        candidates.extend(files.clone());
    }

    if let Some(ref file) = opts.alert_file {
        candidates.push(file.clone());
    }

    candidates
}

fn read_last_failure(alert_files: &[String]) -> Option<LastFailureInfo> {
    for alert_file in alert_files {
        let path = Path::new(alert_file);
        if !path.exists() {
            continue;
        }

        match fs::read_to_string(path) {
            Ok(content) => {
                let lines: Vec<&str> = content.lines().collect();
                if lines.is_empty() {
                    continue;
                }

                // 第一行是时间戳
                if let Ok(timestamp_ms) = lines[0].parse::<i64>() {
                    let failure_time = DateTime::from_timestamp_millis(timestamp_ms)
                        .unwrap_or_else(Utc::now);
                    let now = Utc::now();
                    let age_ms = (now.timestamp_millis() - failure_time.timestamp_millis()) as f64;
                    let age_hours = age_ms / (1000.0 * 60.0 * 60.0);

                    let message = if lines.len() > 1 {
                        lines[1..].join("\n")
                    } else {
                        "Backup failure recorded".to_string()
                    };

                    return Some(LastFailureInfo {
                        age_hours: round_hours(age_hours),
                        message,
                    });
                }
            }
            Err(_) => continue,
        }
    }

    None
}

fn find_latest_backup(backup_dir: &str, now_ms: i64) -> Option<LatestBackupInfo> {
    let dir = Path::new(backup_dir);
    if !dir.exists() || !dir.is_dir() {
        return None;
    }

    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return None,
    };

    let mut latest: Option<(PathBuf, u64, i64)> = None;

    for entry in entries.flatten() {
        let path = entry.path();
        let filename = path.file_name()?.to_str()?;

        // 只考虑 .sql 或 .sql.gz 文件
        if !filename.ends_with(".sql") && !filename.ends_with(".sql.gz") {
            continue;
        }

        if let Ok(metadata) = fs::metadata(&path) {
            if metadata.is_file() {
                let modified = metadata.modified().ok()?;
                let modified_ms = modified
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()?
                    .as_millis() as i64;
                let size = metadata.len();

                match latest {
             None => latest = Some((path, size, modified_ms)),
                    Some((_, _, latest_ms)) if modified_ms > latest_ms => {
                        latest = Some((path, size, modified_ms));
                    }
                    _ => {}
                }
            }
        }
    }

    latest.map(|(path, size, modified_ms)| {
        let age_ms = (now_ms - modified_ms) as f64;
        let age_hours = age_ms / (1000.0 * 60.0 * 60.0);

        LatestBackupInfo {
            path: path.to_string_lossy().to_string(),
            age_hours: round_hours(age_hours),
            size_bytes: size,
        }
    })
}

/// 检查数据库备份健康状态
pub fn inspect_database_backup_health(
    opts: InspectDatabaseBackupHealthOptions,
) -> DatabaseBackupHealthStatus {
    if !opts.enabled {
        return DatabaseBackupHealthStatus {
            enabled: false,
            status: BackupStatus::Ok,
            backup_dir: opts.backup_dir,
            max_age_hours: opts.max_age_hours,
            latest_backup: None,
            last_failure: None,
            warnings: Vec::new(),
        };
    }

    let now = opts.now.unwrap_or_else(Utc::now);
    let now_ms = now.timestamp_millis();

    let mut warnings = Vec::new();

    // 检查最后失败记录
    let alert_files = alert_file_candidates(&opts);
    let last_failure = if !alert_files.is_empty() {
        read_last_failure(&alert_files)
    } else {
        None
    };

    if let Some(ref failure) = last_failure {
        warnings.push(DatabaseBackupHealthWarning {
            code: DatabaseBackupHealthWarningCode::DatabaseBackupLastFailure,
            message: format!(
                "Last backup failure {:.1}h ago: {}",
                failure.age_hours, failure.message
            ),
        });
    }

    // 查找最新备份
    let latest_backup = find_latest_backup(&opts.backup_dir, now_ms);

    match &latest_backup {
        None => {
            warnings.push(DatabaseBackupHealthWarning {
                code: DatabaseBackupHealthWarningCode::DatabaseBackupMissing,
                message: format!("No backup files found in {}", opts.backup_dir),
            });
        }
        Some(backup) => {
            if backup.age_hours > opts.max_age_hours {
                warnings.push(DatabaseBackupHealthWarning {
                    code: DatabaseBackupHealthWarningCode::DatabaseBackupStale,
                    message: format!(
                        "Latest backup is {:.1}h old (max: {:.1}h)",
                        backup.age_hours, opts.max_age_hours
                    ),
                });
            }
        }
    }

    let status = if warnings.is_empty() {
        BackupStatus::Ok
    } else {
        BackupStatus::Warning
    };

    DatabaseBackupHealthStatus {
        enabled: true,
        status,
        backup_dir: opts.backup_dir,
        max_age_hours: opts.max_age_hours,
        latest_backup,
        last_failure,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_hours() {
        assert_eq!(round_hours(1.234), 1.2);
        assert_eq!(round_hours(1.256), 1.3);
        assert_eq!(round_hours(0.0), 0.0);
    }

    #[test]
    fn test_inspect_disabled() {
        let opts = InspectDatabaseBackupHealthOptions {
            enabled: false,
            backup_dir: "/tmp/backups".to_string(),
            max_age_hours: 24.0,
            alert_file: None,
            alert_files: None,
            now: None,
        };

        let status = inspect_database_backup_health(opts);
        assert!(!status.enabled);
        assert_eq!(status.status, BackupStatus::Ok);
        assert!(status.warnings.is_empty());
    }
    #[test]
    fn test_inspect_missing_backup() {
        let opts = InspectDatabaseBackupHealthOptions {
            enabled: true,
            backup_dir: "/nonexistent/path".to_string(),
            max_age_hours: 24.0,
            alert_file: None,
            alert_files: None,
            now: None,
        };

        let status = inspect_database_backup_health(opts);
        assert!(status.enabled);
        assert_eq!(status.status, BackupStatus::Warning);
        assert_eq!(status.warnings.len(), 1);
        assert_eq!(
            status.warnings[0].code,
            DatabaseBackupHealthngCode::DatabaseBackupMissing
        );
    }
}
