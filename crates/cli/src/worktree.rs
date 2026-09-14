//! Safe, local git-worktree lifecycle helpers used by the CLI.
//!
//! The server does not own a checkout, so worktree management belongs in the
//! client.  These commands intentionally operate through `git worktree` and
//! keep all paths explicit; they do not mutate the current branch.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::{env, path::{Path, PathBuf}, process::Command};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorktreeEntry {
    pub path: String,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub detached: bool,
    pub bare: bool,
}

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str).unwrap_or("list") {
        "list" | "ls" => list_command(args),
        "status" => status_command(args),
        "create" | "make" => create_command(args),
        "remove" | "rm" | "cleanup" => remove_command(args),
        "env" => env_command(args),
        _ => {
            println!("Usage: parrot worktree list [--json] | status | create <name> [--path PATH] [--start-point REF] | remove <path-or-name> [--force] | env [--json]");
            Ok(())
        }
    }
}

fn list_command(args: &[String]) -> Result<()> {
    let cwd = current_dir()?;
    let entries = list_worktrees(&cwd)?;
    if args.iter().any(|arg| arg == "--json") {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else if entries.is_empty() {
        println!("no git worktrees found");
    } else {
        for entry in entries {
            let branch = entry
                .branch
                .as_deref()
                .unwrap_or(if entry.detached { "(detached)" } else { "(bare)" });
            println!("{}\t{}\t{}", entry.path, entry.head.as_deref().unwrap_or("-"), branch);
        }
    }
    Ok(())
}

fn status_command(_args: &[String]) -> Result<()> {
    let cwd = current_dir()?;
    let root = git_output(&cwd, &["rev-parse", "--show-toplevel"])?;
    let entries = list_worktrees(&cwd)?;
    let current = entries.iter().find(|entry| entry.path == root);
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "repository": root,
            "current": current,
            "worktreeCount": entries.len(),
        }))?
    );
    Ok(())
}

fn create_command(args: &[String]) -> Result<()> {
    let name = args
        .get(1)
        .filter(|value| !value.starts_with('-'))
        .ok_or_else(|| anyhow::anyhow!("Usage: parrot worktree create <name> [--path PATH] [--start-point REF]"))?;
    validate_name(name)?;
    let cwd = current_dir()?;
    let path = get_flag_value(args, "--path")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            cwd.join(".parrot-worktrees")
                .join(name)
        });
    if path.exists() {
        bail!("worktree path already exists: {}", path.display());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let start_point = get_flag_value(args, "--start-point").unwrap_or_else(|| "HEAD".to_owned());
    let branch = get_flag_value(args, "--branch").unwrap_or_else(|| format!("parrot/{name}"));
    validate_branch(&branch)?;
    let path_string = path.to_string_lossy().to_string();
    let output = git_command(
        &cwd,
        vec![
            "worktree",
            "add",
            "-b",
            branch.as_str(),
            path_string.as_str(),
            start_point.as_str(),
        ],
    )?;
    if !output.trim().is_empty() {
        print!("{output}");
        if !output.ends_with('\n') {
            println!();
        }
    }
    println!("created worktree {} at {}", name, path.display());
    Ok(())
}

fn remove_command(args: &[String]) -> Result<()> {
    let target = args
        .get(1)
        .ok_or_else(|| anyhow::anyhow!("Usage: parrot worktree remove <path-or-name> [--force]"))?;
    let cwd = current_dir()?;
    let entries = list_worktrees(&cwd)?;
    let path = resolve_target_path(target, &entries)?;
    let mut git_args = vec!["worktree", "remove"];
    if args.iter().any(|arg| arg == "--force") {
        git_args.push("--force");
    }
    let path_string = path.to_string_lossy().to_string();
    git_args.push(&path_string);
    let output = git_command(&cwd, git_args)?;
    if !output.trim().is_empty() {
        print!("{output}");
    }
    println!("removed worktree {}", path.display());
    Ok(())
}

fn env_command(args: &[String]) -> Result<()> {
    let cwd = current_dir()?;
    let root = git_output(&cwd, &["rev-parse", "--show-toplevel"])?;
    let payload = serde_json::json!({
        "PARROT_IN_WORKTREE": "true",
        "PARROT_WORKTREE_ROOT": root,
        "PARROT_WORKTREE_CWD": cwd,
    });
    if args.iter().any(|arg| arg == "--json") {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("export PARROT_IN_WORKTREE=true");
        println!("export PARROT_WORKTREE_ROOT={}", shell_quote(payload["PARROT_WORKTREE_ROOT"].as_str().unwrap_or_default()));
        println!("export PARROT_WORKTREE_CWD={}", shell_quote(payload["PARROT_WORKTREE_CWD"].as_str().unwrap_or_default()));
    }
    Ok(())
}

pub fn list_worktrees(cwd: &Path) -> Result<Vec<WorktreeEntry>> {
    let output = git_output(cwd, &["worktree", "list", "--porcelain"])?;
    let mut entries = Vec::new();
    let mut current: Option<WorktreeEntry> = None;
    for line in output.lines() {
        if line.is_empty() {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            continue;
        }
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(WorktreeEntry {
                path: path.to_owned(),
                head: None,
                branch: None,
                detached: false,
                bare: false,
            });
        } else if let Some(entry) = current.as_mut() {
            if let Some(head) = line.strip_prefix("HEAD ") {
                entry.head = Some(head.to_owned());
            } else if let Some(branch) = line.strip_prefix("branch ") {
                entry.branch = Some(branch.strip_prefix("refs/heads/").unwrap_or(branch).to_owned());
            } else if line == "detached" {
                entry.detached = true;
            } else if line == "bare" {
                entry.bare = true;
            }
        }
    }
    if let Some(entry) = current {
        entries.push(entry);
    }
    Ok(entries)
}

fn resolve_target_path(target: &str, entries: &[WorktreeEntry]) -> Result<PathBuf> {
    let direct = PathBuf::from(target);
    if entries.iter().any(|entry| Path::new(&entry.path) == direct) {
        return Ok(direct);
    }
    let matches = entries
        .iter()
        .filter(|entry| {
            Path::new(&entry.path)
                .file_name()
                .and_then(|value| value.to_str())
                == Some(target)
                || entry.branch.as_deref() == Some(target)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [entry] => Ok(PathBuf::from(&entry.path)),
        [] => bail!("worktree not found: {target}"),
        _ => bail!("worktree name is ambiguous: {target}; use its path"),
    }
}

fn current_dir() -> Result<PathBuf> {
    env::current_dir().context("resolve current working directory")
}

fn git_output(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to execute git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn git_command<'a, I>(cwd: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = &'a str>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    let output = Command::new("git")
        .args(&args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to execute git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn get_flag_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|window| window[0] == flag).map(|window| window[1].clone())
}

fn validate_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        bail!("worktree name must be a single non-empty path component");
    }
    Ok(())
}

fn validate_branch(branch: &str) -> Result<()> {
    if branch.trim().is_empty() || branch.starts_with('-') {
        bail!("worktree branch must not be empty or start with '-'");
    }
    Ok(())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::list_worktrees;

    #[test]
    fn parses_porcelain_worktree_output() {
        let dir = tempfile::tempdir().unwrap();
        std::process::Command::new("git").args(["init", "-q"]).current_dir(dir.path()).status().unwrap();
        std::process::Command::new("git").args(["-c", "user.email=test@example.com", "-c", "user.name=Test", "commit", "--allow-empty", "-m", "init", "-q"]).current_dir(dir.path()).status().unwrap();
        let entries = list_worktrees(dir.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].branch.as_deref(), Some("master") | Some("main")));
    }
}
