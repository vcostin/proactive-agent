//! System prerequisite detection: detect + suggest (no package-manager automation).
//!
//! Windows VCRedist `install_vcredist` remains an explicit narrow exception —
//! see `install_helper_exception` on that row — not the default policy.

use serde::Serialize;

/// Structured result for one system prerequisite row shown in Setup Wizard / repair.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PrerequisiteCheck {
    pub id: String,
    pub label: String,
    pub status: PrerequisiteStatus,
    /// Present when status is Missing (or Degraded): how the user might install it.
    pub guidance: Option<String>,
    /// True when this row is only meaningful on the current platform.
    pub applicable: bool,
    /// Non-empty when this row still offers an install helper (documented exception).
    pub install_helper_exception: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrerequisiteStatus {
    Ok,
    Missing,
    /// Detected but may be insufficient (e.g. VCRedist present, llama still fails).
    Degraded,
    /// Not relevant on this Host/Guest OS — UI should hide, not show as failure.
    NotApplicable,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PrerequisiteReport {
    pub platform: String,
    pub checks: Vec<PrerequisiteCheck>,
}

/// Inputs for prerequisite evaluation (injectable for tests).
#[derive(Debug, Clone)]
pub struct PrerequisiteInputs {
    pub platform: String,
    pub vcredist_present: bool,
    pub vulkan_ok: bool,
    pub llama_server_ok: bool,
    pub llama_server_msg: String,
}

pub fn evaluate_prerequisites(input: &PrerequisiteInputs) -> PrerequisiteReport {
    let mut checks = Vec::new();

    // Visual C++ — Windows only
    let vcredist_applicable = input.platform == "windows";
    if vcredist_applicable {
        let (status, guidance) = if input.vcredist_present && input.llama_server_ok {
            (PrerequisiteStatus::Ok, None)
        } else if input.vcredist_present && !input.llama_server_ok {
            (
                PrerequisiteStatus::Degraded,
                Some(
                    "Visual C++ Runtime is present but llama-server still fails to start. \
                     Update VCRedist 2022 x64 from Microsoft, or use the optional in-app helper \
                     (documented exception to detect+suggest)."
                        .into(),
                ),
            )
        } else {
            (
                PrerequisiteStatus::Missing,
                Some(
                    "Install Visual C++ Redistributable 2022 x64 from \
                     https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist \
                     (or use the optional in-app helper — documented exception)."
                        .into(),
                ),
            )
        };
        checks.push(PrerequisiteCheck {
            id: "vcredist".into(),
            label: "Visual C++ Runtime 2022".into(),
            status,
            guidance,
            applicable: true,
            install_helper_exception: Some(
                "install_vcredist downloads and silently runs the Microsoft VCRedist installer. \
                 Prefer detect+suggest; helper retained temporarily for Windows Guest path."
                    .into(),
            ),
        });
    }

    // Vulkan / GPU — all platforms; macOS treated as Metal-ok via vulkan_ok flag
    let gpu_label = if input.platform == "macos" {
        "Metal / GPU"
    } else {
        "Vulkan Runtime"
    };
    let gpu_guidance = match input.platform.as_str() {
        "macos" => {
            "GPU stack not detected. Ensure macOS Metal is available (system default)."
        }
        "windows" => {
            "Install or update GPU drivers (AMD / Nvidia / Intel) that include Vulkan support."
        }
        _ => {
            "Install a Vulkan driver package for your GPU, e.g. mesa-vulkan-drivers, amdvlk, \
             or the Nvidia proprietary driver. The Setup Wizard will not run a package manager."
        }
    };
    checks.push(PrerequisiteCheck {
        id: "vulkan".into(),
        label: gpu_label.into(),
        status: if input.vulkan_ok {
            PrerequisiteStatus::Ok
        } else {
            PrerequisiteStatus::Missing
        },
        guidance: if input.vulkan_ok {
            None
        } else {
            Some(gpu_guidance.into())
        },
        applicable: true,
        install_helper_exception: None,
    });

    // llama-server smoke test
    checks.push(PrerequisiteCheck {
        id: "llama_server".into(),
        label: "llama-server binary".into(),
        status: if input.llama_server_ok {
            PrerequisiteStatus::Ok
        } else {
            PrerequisiteStatus::Missing
        },
        guidance: if input.llama_server_ok {
            None
        } else {
            Some(format!(
                "{}. Open Setup repair to re-download app-managed artifacts if the binary is missing.",
                input.llama_server_msg
            ))
        },
        applicable: true,
        install_helper_exception: None,
    });

    PrerequisiteReport {
        platform: input.platform.clone(),
        checks,
    }
}

/// Live system check used by the Tauri command (and Setup repair on open).
/// Callers supply detection results so this module stays free of command-layer deps.
pub fn report_from_live(
    platform: &str,
    vcredist_present: bool,
    vulkan_ok: bool,
    llama_server_ok: bool,
    llama_server_msg: String,
) -> PrerequisiteReport {
    evaluate_prerequisites(&PrerequisiteInputs {
        platform: platform.to_string(),
        vcredist_present,
        vulkan_ok,
        llama_server_ok,
        llama_server_msg,
    })
}

/// Legacy `SystemDeps` shape kept for existing UI during expand.
#[derive(Debug, Clone, Serialize)]
pub struct SystemDeps {
    pub platform: String,
    pub vcredist_ok: bool,
    pub vulkan_ok: bool,
    pub llama_server_ok: bool,
    pub llama_server_msg: String,
    /// Structured detect+suggest rows (platform-aware).
    pub prerequisites: Vec<PrerequisiteCheck>,
}

impl SystemDeps {
    pub fn from_report(report: PrerequisiteReport, llama_server_msg: String) -> Self {
        let vcredist_ok = report
            .checks
            .iter()
            .find(|c| c.id == "vcredist")
            .map(|c| matches!(c.status, PrerequisiteStatus::Ok))
            .unwrap_or(true);
        let vulkan_ok = report
            .checks
            .iter()
            .find(|c| c.id == "vulkan")
            .map(|c| matches!(c.status, PrerequisiteStatus::Ok))
            .unwrap_or(false);
        let llama_server_ok = report
            .checks
            .iter()
            .find(|c| c.id == "llama_server")
            .map(|c| matches!(c.status, PrerequisiteStatus::Ok))
            .unwrap_or(false);
        Self {
            platform: report.platform,
            vcredist_ok,
            vulkan_ok,
            llama_server_ok,
            llama_server_msg,
            prerequisites: report.checks,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_hides_vcredist_row_and_suggests_vulkan() {
        let report = evaluate_prerequisites(&PrerequisiteInputs {
            platform: "linux".into(),
            vcredist_present: false,
            vulkan_ok: false,
            llama_server_ok: true,
            llama_server_msg: "ok".into(),
        });
        assert!(report.checks.iter().all(|c| c.id != "vcredist"));
        let vulkan = report.checks.iter().find(|c| c.id == "vulkan").unwrap();
        assert_eq!(vulkan.status, PrerequisiteStatus::Missing);
        assert!(vulkan
            .guidance
            .as_ref()
            .unwrap()
            .contains("mesa-vulkan-drivers"));
        assert!(vulkan.install_helper_exception.is_none());
    }

    #[test]
    fn windows_vcredist_missing_includes_guidance_and_exception_note() {
        let report = evaluate_prerequisites(&PrerequisiteInputs {
            platform: "windows".into(),
            vcredist_present: false,
            vulkan_ok: true,
            llama_server_ok: false,
            llama_server_msg: "DLL not found".into(),
        });
        let vc = report.checks.iter().find(|c| c.id == "vcredist").unwrap();
        assert_eq!(vc.status, PrerequisiteStatus::Missing);
        assert!(vc.guidance.is_some());
        assert!(vc.install_helper_exception.is_some());
    }

    #[test]
    fn macos_does_not_surface_vcredist_failure() {
        let report = evaluate_prerequisites(&PrerequisiteInputs {
            platform: "macos".into(),
            vcredist_present: false,
            vulkan_ok: true,
            llama_server_ok: true,
            llama_server_msg: "ok".into(),
        });
        assert!(report.checks.iter().all(|c| c.id != "vcredist"));
        assert!(report
            .checks
            .iter()
            .all(|c| c.status == PrerequisiteStatus::Ok));
    }
}
