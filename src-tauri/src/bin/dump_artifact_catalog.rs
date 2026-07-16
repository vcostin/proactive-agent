//! Dump the current (or requested) Platform-module artifact catalog as JSON.
//!
//! Used by Developer setup to stay aligned with the Rust SSOT:
//!   cargo run --bin dump_artifact_catalog -- --platform linux > scripts/artifacts/linux.json

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let platform = args
        .iter()
        .position(|a| a == "--platform")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or_else(|| {
            if cfg!(target_os = "windows") {
                "windows"
            } else if cfg!(target_os = "macos") {
                "macos"
            } else {
                "linux"
            }
        });

    let artifacts = match platform {
        "linux" => proactive_agent_lib::platform::linux::ARTIFACTS,
        "windows" => proactive_agent_lib::platform::windows::ARTIFACTS,
        "macos" => proactive_agent_lib::platform::macos::ARTIFACTS,
        other => {
            eprintln!("unknown platform: {other} (expected linux|windows|macos)");
            return ExitCode::FAILURE;
        }
    };

    let projection =
        proactive_agent_lib::platform::artifact::project_catalog(platform, artifacts);
    match serde_json::to_string_pretty(&projection) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("serialize failed: {e}");
            ExitCode::FAILURE
        }
    }
}
