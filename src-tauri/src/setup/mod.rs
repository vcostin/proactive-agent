//! Setup Wizard / Setup repair command-surface helpers.
//!
//! Primary seam for status, Core agent readiness, and (with `platform`)
//! app-managed artifact verification. Tests target these modules — not UI e2e.

pub mod prerequisites;
pub mod status;

pub use prerequisites::{
    evaluate_prerequisites, report_from_live, PrerequisiteCheck, PrerequisiteInputs,
    PrerequisiteReport, PrerequisiteStatus, SystemDeps,
};
pub use status::{build_setup_status, check_binaries_in, derive_setup_status};
