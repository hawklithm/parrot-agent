use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

#[derive(Debug, Serialize)]
struct BackupReport {
    backup_file: String,
    size_bytes: u64,
    pruned_count: u32,
    backup_dir: String,
    retention_days: u64,
    connection_source: String,
}

pub fn run(args: &[String]) -> Result<()> {
    let mut connection_string = None;
    let mut backup_dir = None;
    let mut retention_days = None;
    let mut json_output = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--connection-string" => {
                connection_string = Some(argument_value(args, index, "--connection-string")?);
                index += 2;
            }
            "--dir" => {
                backup_dir = Some(PathBuf::from(argument_value(args, index, "--dir")?));
                index += 2;
            }
            "--retention-days" => {
                retention_days = Some(
                    argument_value(args, index, "--retention-days")?
                        .parse::<u64>()
                        .context("--retention-days must be a positive integer")?,
                );
                index += 2;
            }
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => bail!("unknown db-backup option '{flag}'"),
        }
    }

    let (connection_string, connection_source) = connection_string
        .map(|value| (value, "--connection-string".to_string()))
        .or_else(|| {
            std::env::var("PARROT_DATABASE_URL")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .map(|value| (value, "PARROT_DATABASE_URL".to_string()))
        })
        .or_else(|| {
            std::env::var("DATABASE_URL")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .map(|value| (value, "DATABASE_URL".to_string()))
        })
        .ok_or_else(|| anyhow::anyhow!("database connection is required via --connection-string, PARROT_DATABASE_URL, or DATABASE_URL"))?;
    let retention_days = retention_days
        .or_else(|| {
            std::env::var("PARROT_DATABASE_BACKUP_RETENTION_DAYS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
        })
        .unwrap_or(30);
    if retention_days == 0 {
        bail!("--retention-days must be a positive integer");
    }
    let backup_dir = backup_dir
        .or_else(|| std::env::var_os("PARROT_DATABASE_BACKUP_DIR").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("data/backups"));
    fs::create_dir_all(&backup_dir)
        .with_context(|| format!("failed to create backup directory {}", backup_dir.display()))?;

    let backup_path = backup_dir.join(format!(
        "parrot-{}-{}.sql",
        chrono::Local::now().format("%Y%m%d-%H%M%S"),
        Uuid::new_v4().simple()
    ));
    let pg_dump = std::env::var("PARROT_PG_DUMP").unwrap_or_else(|_| "pg_dump".to_string());
    let output = Command::new(&pg_dump)
        .args(["--no-owner", "--no-privileges", "--format=plain", "--file"])
        .arg(&backup_path)
        .arg(&connection_string)
        .output()
        .with_context(|| format!("failed to start {pg_dump}"))?;
    if !output.status.success() {
        let _ = fs::remove_file(&backup_path);
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!(
            "pg_dump exited with code {}{}",
            output.status.code().unwrap_or(-1),
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        );
    }
    let size_bytes = fs::metadata(&backup_path)
        .with_context(|| format!("failed to stat {}", backup_path.display()))?
        .len();
    let pruned_count = prune_backup_files(&backup_dir, retention_days)?;
    let report = BackupReport {
        backup_file: backup_path.to_string_lossy().to_string(),
        size_bytes,
        pruned_count,
        backup_dir: backup_dir.to_string_lossy().to_string(),
        retention_days,
        connection_source,
    };
    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("backup_file: {}", report.backup_file);
        println!("size_bytes: {}", report.size_bytes);
        println!("pruned_count: {}", report.pruned_count);
        println!("retention_days: {}", report.retention_days);
        println!("connection_source: {}", report.connection_source);
        println!("status: completed");
    }
    Ok(())
}

fn argument_value(args: &[String], index: usize, flag: &str) -> Result<String> {
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("missing value for {flag}"))
}

fn prune_backup_files(directory: &Path, retention_days: u64) -> Result<u32> {
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(retention_days.saturating_mul(86_400)))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut removed = 0;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("parrot-") || !name.ends_with(".sql") {
            continue;
        }
        let metadata = entry.metadata()?;
        if metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH) < cutoff {
            fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}
