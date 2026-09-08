//! Parrot CLI managed install store.
//!
//! Tracks installed versions, payload paths, and channel state so that
//! `update` and `uninstall` can operate correctly even when the CLI is
//! invoked from a symlink or user-local copy.
//!
//! Adapted from Paperclip's install-store.ts; simplified for Rust.

use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Version of the manifest schema. Bump this when the JSON shape changes.
pub const INSTALL_MANIFEST_VERSION: u64 = 1;

/// Marker written into the PATH block of shell rc files.
pub const PATH_BLOCK_START: &str = "# >>> parrot managed PATH >>>";
pub const PATH_BLOCK_END: &str = "# <<< parrot managed PATH <<<";
pub const MANAGED_STORE_MARKER: &str = "parrot managed install store v1\n";

/// Channel from which the CLI was installed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallChannel {
    Latest,
    Canary,
    Pinned(String),
}

impl InstallChannel {
    pub fn as_str(&self) -> &str {
        match self {
            InstallChannel::Latest => "latest",
            InstallChannel::Canary => "canary",
            InstallChannel::Pinned(v) => v,
        }
    }
}

impl std::fmt::Display for InstallChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A single installed record (one source + version).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstallRecord {
    /// Source: "local" (copied binary) or "download" (fetched from releases).
    pub source: String,
    /// Semantic version string.
    pub version: String,
    /// Channel from which it was installed.
    pub channel: InstallChannel,
    /// Absolute path to the binary.
    pub executable_path: PathBuf,
    /// Timestamp when installed (seconds since epoch).
    pub installed_at: u64,
}

/// The top-level manifest stored at `~/.local/share/parrot/install-manifest.json`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstallManifest {
    pub version: u64,
    pub current: InstallRecord,
    /// Previous record, retained for rollback.
    pub previous: Option<InstallRecord>,
}

/// Paths used by the install store.
#[derive(Debug, Clone)]
pub struct InstallStorePaths {
    pub manifest_path: PathBuf,
    pub state_dir: PathBuf,
}

impl InstallStorePaths {
    pub fn new() -> Self {
        let state_dir = resolve_state_dir();
        let manifest_path = state_dir.join("install-manifest.json");
        Self {
            manifest_path,
            state_dir,
        }
    }
}

impl Default for InstallStorePaths {
    fn default() -> Self {
        Self::new()
    }
}

fn resolve_state_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("parrot");
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".local/share/parrot")
}

/// Read the current manifest, if one exists.
pub fn read_install_manifest(paths: &InstallStorePaths) -> Result<Option<InstallManifest>> {
    if !paths.manifest_path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&paths.manifest_path)
        .with_context(|| format!("read manifest at {:?}", paths.manifest_path))?;
    let manifest: InstallManifest =
        serde_json::from_str(&content).with_context(|| "parse install manifest JSON")?;
    if manifest.version != INSTALL_MANIFEST_VERSION {
        bail!(
            "unsupported manifest version {}: expected {}",
            manifest.version,
            INSTALL_MANIFEST_VERSION
        );
    }
    Ok(Some(manifest))
}

/// Write the manifest atomically (write to temp, then rename).
pub fn write_install_manifest_atomic(
    manifest: &InstallManifest,
    paths: &InstallStorePaths,
) -> Result<()> {
    ensure_private_directory(&paths.state_dir)?;
    let tmp = paths.manifest_path.with_extension("tmp");
    let content = serde_json::to_string_pretty(manifest)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    fs::rename(&tmp, &paths.manifest_path)
        .with_context(|| format!("atomic rename manifest to {:?}", paths.manifest_path))?;
    Ok(())
}

/// Build a new manifest given a record and optional existing manifest.
pub fn build_next_manifest(
    record: InstallRecord,
    current: Option<&InstallManifest>,
) -> InstallManifest {
    let previous = current.map(|m| m.current.clone());
    InstallManifest {
        version: INSTALL_MANIFEST_VERSION,
        current: record,
        previous,
    }
}

/// Check whether a given executable path matches the current manifest.
pub fn is_managed_executable(
    executable_path: &Path,
    manifest: &InstallManifest,
) -> bool {
    executable_path
        .canonicalize()
        .ok()
        .map(|p| p == manifest.current.executable_path.canonicalize().unwrap_or_default())
        .unwrap_or(false)
}

/// Ensure the state directory exists and is owned by the current user.
fn ensure_private_directory(dir: &Path) -> Result<()> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Resolve the default CLI install directory for the current platform.
pub fn default_cli_bin_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".local/bin")
}

/// Get the path of the currently running executable.
pub fn current_exe_path() -> Result<PathBuf> {
    std::env::current_exe()
        .context("resolve current executable path")
}

/// Detect whether the current executable was installed via the managed store.
pub fn detect_install_mode(paths: &InstallStorePaths) -> Result<InstallMode> {
    let manifest = match read_install_manifest(paths) {
        Ok(Some(m)) => m,
        Ok(None) | Err(_) => return Ok(InstallMode::Unknown),
    };
    let current = current_exe_path()?;
    if is_managed_executable(&current, &manifest) {
        Ok(InstallMode::Managed)
    } else {
        Ok(InstallMode::Unknown)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallMode {
    Managed,
    Unknown,
}

/// Remove the manifest (used by uninstall).
pub fn remove_install_manifest(paths: &InstallStorePaths) -> Result<bool> {
    if paths.manifest_path.exists() {
        fs::remove_file(&paths.manifest_path)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Roll back to the previous manifest entry, if available.
pub fn rollback_manifest(paths: &InstallStorePaths) -> Result<InstallManifest> {
    let manifest = read_install_manifest(paths)?
        .context("no manifest found for rollback")?;
    let previous = manifest
        .previous
        .context("no previous version to roll back to")?;
    Ok(InstallManifest {
        version: INSTALL_MANIFEST_VERSION,
        current: previous,
        previous: None,
    })
}

/// Print the current install state as JSON.
pub fn print_manifest_json(paths: &InstallStorePaths, pretty: bool) -> Result<()> {
    match read_install_manifest(paths)? {
        Some(manifest) => {
            if pretty {
                println!("{}", serde_json::to_string_pretty(&manifest)?);
            } else {
                println!("{}", serde_json::to_string(&manifest)?);
            }
        }
        None => {
            println!("{{}}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn make_tmp_manifest() -> (InstallStorePaths, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let paths = InstallStorePaths {
            manifest_path: dir.path().join("install-manifest.json"),
            state_dir: dir.path().join("state"),
        };
        (paths, dir)
    }

    #[test]
    fn test_write_and_read_manifest() {
        let (paths, _dir) = make_tmp_manifest();
        let record = InstallRecord {
            source: "local".to_string(),
            version: "1.0.0".to_string(),
            channel: InstallChannel::Latest,
            executable_path: PathBuf::from("/usr/local/bin/parrot"),
            installed_at: 1000,
        };
        let manifest = build_next_manifest(record, None);
        write_install_manifest_atomic(&manifest, &paths).unwrap();

        let read = read_install_manifest(&paths).unwrap().unwrap();
        assert_eq!(read.current.version, "1.0.0");
        assert_eq!(read.current.source, "local");
    }

    #[test]
    fn test_previous_is_preserved() {
        let (paths, _dir) = make_tmp_manifest();
        let r1 = InstallRecord {
            source: "local".to_string(),
            version: "1.0.0".to_string(),
            channel: InstallChannel::Latest,
            executable_path: PathBuf::from("/usr/local/bin/parrot"),
            installed_at: 1000,
        };
        let m1 = build_next_manifest(r1.clone(), None);
        write_install_manifest_atomic(&m1, &paths).unwrap();

        let r2 = InstallRecord {
            source: "download".to_string(),
            version: "1.1.0".to_string(),
            channel: InstallChannel::Latest,
            executable_path: PathBuf::from("/usr/local/bin/parrot"),
            installed_at: 2000,
        };
        let m2 = build_next_manifest(r2, Some(&m1));
        write_install_manifest_atomic(&m2, &paths).unwrap();

        let read = read_install_manifest(&paths).unwrap().unwrap();
        assert_eq!(read.current.version, "1.1.0");
        assert_eq!(read.previous.as_ref().unwrap().version, "1.0.0");
    }

    #[test]
    fn test_remove_manifest() {
        let (paths, _dir) = make_tmp_manifest();
        let record = InstallRecord {
            source: "local".to_string(),
            version: "1.0.0".to_string(),
            channel: InstallChannel::Latest,
            executable_path: PathBuf::from("/usr/local/bin/parrot"),
            installed_at: 1000,
        };
        let manifest = build_next_manifest(record, None);
        write_install_manifest_atomic(&manifest, &paths).unwrap();
        assert!(paths.manifest_path.exists());

        let removed = remove_install_manifest(&paths).unwrap();
        assert!(removed);
        assert!(!paths.manifest_path.exists());
    }

    fn test_is_managed_executable() {
        let (_paths, dir) = make_tmp_manifest();
        // `is_managed_executable` canonicalizes both sides, so the fixture must
        // point at a path that actually exists.
        let managed = dir.path().join("parrot");
        std::fs::write(&managed, b"binary").unwrap();
        let other = dir.path().join("other-parrot");
        std::fs::write(&other, b"binary").unwrap();

        let record = InstallRecord {
            source: "local".to_string(),
            version: "1.0.0".to_string(),
            channel: InstallChannel::Latest,
            executable_path: managed.clone(),
            installed_at: 1000,
        };
        let manifest = build_next_manifest(record, None);

        assert!(is_managed_executable(&managed, &manifest));
        assert!(!is_managed_executable(&other, &manifest));
    }
}
