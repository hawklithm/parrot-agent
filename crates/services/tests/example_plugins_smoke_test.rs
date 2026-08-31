//! §6B.2: Example plugin manifests satisfy the Paperclip plugin contract.
//!
//! Parrot ships the Paperclip reference example plugins (translated to JSON
//! fixtures under `examples/plugins/*.manifest.json`). This smoke test asserts
//! every example manifest deserializes into `PluginManifestV1` and passes
//! `validate_manifest` — the same validation the plugin loader enforces at
//! install time. Any drift from Paperclip's example set (capabilities,
//! categories, entrypoints, ui slots, tools, jobs, webhooks, environment
//! drivers) fails here, mirroring Paperclip's author smoke gate.

use std::path::PathBuf;

use serde_json::Value;
use services::plugin_capabilities::validate_manifest;

fn examples_dir() -> PathBuf {
    // crate-root-relative: services crate lives in crates/services; the
    // examples live at crate-root level (crates/.. /examples/plugins).
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")); // crates/services
    dir.pop(); // crates
    dir.pop(); // repo root (parrot-agent)
    dir.push("examples");
    dir.push("plugins");
    dir
}

fn load_example(name: &str) -> Value {
    let path = examples_dir().join(format!("{name}.manifest.json"));
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    serde_json::from_str(&raw).expect("valid JSON fixture")
}

fn example_ids() -> &'static [&'static str] {
    &[
        "plugin-authoring-smoke-example",
        "plugin-file-browser-example",
        "plugin-hello-world-example",
        "plugin-kitchen-sink-example",
        "plugin-orchestration-smoke-example",
    ]
}

#[test]
fn all_example_manifests_deserialize_and_validate() {
    for id in example_ids() {
        let value = load_example(id);
        let manifest: services::plugin_capabilities::PluginManifestV1 =
            serde_json::from_value(value.clone())
                .unwrap_or_else(|e| panic!("deserialize {id}: {e}"));
        validate_manifest(&manifest)
            .unwrap_or_else(|e| panic!("example {id} must satisfy the plugin contract: {e:?}"));
    }
}

#[test]
fn example_ids_are_canonical_lowercase() {
    for id in example_ids() {
        let value = load_example(id);
        let manifest_id = value["id"].as_str().expect("id present");
        assert!(
            manifest_id
                == manifest_id
                    .to_ascii_lowercase()
                && manifest_id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-' || c == '_'),
            "{id}: manifest id {manifest_id:?} is not canonical lowercase"
        );
    }
}

#[test]
fn kitchen_sink_covers_full_capability_surface() {
    let value = load_example("plugin-kitchen-sink-example");
    let caps: Vec<String> = value["capabilities"]
        .as_array()
        .expect("capabilities array")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    // The kitchen sink is the reference surface Paperclip exercises
    // end-to-end: it must declare the read/write issue, agent session, tool
    // registration, job scheduling, and webhook receive capabilities.
    for required in [
        "issues.read",
        "issues.create",
        "issues.update",
        "issue.comments.create",
        "agents.invoke",
        "agent.sessions.create",
        "agent.tools.register",
        "jobs.schedule",
        "webhooks.receive",
    ] {
        assert!(
            caps.iter().any(|c| c == required),
            "kitchen-sink must declare {required}"
        );
    }
}
