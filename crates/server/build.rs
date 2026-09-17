use std::process::Command;

fn main() {
    // Capture git commit hash at build time
    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // Capture build timestamp as ISO 8601
    let build_time = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Capture schema version (max migration number in migrations/)
    let schema_version = "91";

    println!("cargo:rustc-env=BUILD_GIT_COMMIT={}", commit);
    println!("cargo:rustc-env=BUILD_TIME={}", build_time);
    println!("cargo:rustc-env=SCHEMA_VERSION={}", schema_version);
}
