//! Curated Piper voice catalog and installed detection.
//!
//! Lists the locked P0 shortlist with human-readable labels/locale and
//! reports whether each voice’s `.onnx` + `.onnx.json` pair is present
//! under the models/tts directory.

use std::path::Path;

use super::piper_voice::piper_voice_pair_present;

/// Static curated entry (id / label / locale) for the locked P0 shortlist.
struct CuratedMeta {
    id: &'static str,
    label: &'static str,
    locale: &'static str,
}

/// Locked P0 Piper shortlist — order is the picker order.
const CURATED: &[CuratedMeta] = &[
    CuratedMeta {
        id: "en_US-lessac-medium",
        label: "Lessac",
        locale: "en_US",
    },
    CuratedMeta {
        id: "en_US-joe-medium",
        label: "Joe",
        locale: "en_US",
    },
    CuratedMeta {
        id: "en_US-kristin-medium",
        label: "Kristin",
        locale: "en_US",
    },
    CuratedMeta {
        id: "en_US-bryce-medium",
        label: "Bryce",
        locale: "en_US",
    },
    CuratedMeta {
        id: "en_US-sam-medium",
        label: "Sam",
        locale: "en_US",
    },
    CuratedMeta {
        id: "en_GB-cori-medium",
        label: "Cori",
        locale: "en_GB",
    },
];

/// One curated Piper voice as shown to callers (picker, tests, download).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CuratedPiperVoice {
    pub id: String,
    pub label: String,
    pub locale: String,
    pub installed: bool,
}

/// Curated P0 catalog with installed state for files under `tts_dir`.
pub fn list_curated_piper_voices(tts_dir: &Path) -> Vec<CuratedPiperVoice> {
    CURATED
        .iter()
        .map(|meta| CuratedPiperVoice {
            id: meta.id.to_string(),
            label: meta.label.to_string(),
            locale: meta.locale.to_string(),
            installed: piper_voice_pair_present(tts_dir, meta.id),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("pa-piper-catalog-{label}-{nanos}"))
    }

    fn write_voice_pair(tts_dir: &Path, id: &str) {
        fs::create_dir_all(tts_dir).unwrap();
        fs::write(tts_dir.join(format!("{id}.onnx")), b"onnx").unwrap();
        fs::write(tts_dir.join(format!("{id}.onnx.json")), b"{}").unwrap();
    }

    fn entry<'a>(catalog: &'a [CuratedPiperVoice], id: &str) -> &'a CuratedPiperVoice {
        catalog
            .iter()
            .find(|v| v.id == id)
            .unwrap_or_else(|| panic!("missing curated id {id}"))
    }

    #[test]
    fn curated_catalog_exposes_p0_ids_with_labels_and_locale() {
        let root = unique_temp("meta");
        let tts = root.join("tts");
        fs::create_dir_all(&tts).unwrap();

        let catalog = list_curated_piper_voices(&tts);
        let by_id: Vec<_> = catalog.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(
            by_id,
            [
                "en_US-lessac-medium",
                "en_US-joe-medium",
                "en_US-kristin-medium",
                "en_US-bryce-medium",
                "en_US-sam-medium",
                "en_GB-cori-medium",
            ]
        );

        let lessac = entry(&catalog, "en_US-lessac-medium");
        assert_eq!(lessac.label, "Lessac");
        assert_eq!(lessac.locale, "en_US");

        assert_eq!(entry(&catalog, "en_US-joe-medium").label, "Joe");
        assert_eq!(entry(&catalog, "en_US-joe-medium").locale, "en_US");
        assert_eq!(entry(&catalog, "en_US-kristin-medium").label, "Kristin");
        assert_eq!(entry(&catalog, "en_US-bryce-medium").label, "Bryce");
        assert_eq!(entry(&catalog, "en_US-sam-medium").label, "Sam");

        let cori = entry(&catalog, "en_GB-cori-medium");
        assert_eq!(cori.label, "Cori");
        assert_eq!(cori.locale, "en_GB");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_tts_dir_reports_all_voices_available_not_installed() {
        let root = unique_temp("empty");
        let tts = root.join("tts");
        fs::create_dir_all(&tts).unwrap();

        let catalog = list_curated_piper_voices(&tts);
        assert!(
            catalog.iter().all(|v| !v.installed),
            "no voice pair present → none installed"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn default_lessac_reports_installed_when_pair_present() {
        let root = unique_temp("lessac");
        let tts = root.join("tts");
        write_voice_pair(&tts, "en_US-lessac-medium");

        let catalog = list_curated_piper_voices(&tts);
        assert!(
            entry(&catalog, "en_US-lessac-medium").installed,
            "lessac onnx+json should count as installed"
        );
        assert!(
            !entry(&catalog, "en_US-joe-medium").installed,
            "other curated voices stay available-only"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn installed_requires_both_onnx_and_json() {
        let root = unique_temp("onnx-only");
        let tts = root.join("tts");
        fs::create_dir_all(&tts).unwrap();
        fs::write(tts.join("en_US-joe-medium.onnx"), b"onnx").unwrap();

        let catalog = list_curated_piper_voices(&tts);
        assert!(
            !entry(&catalog, "en_US-joe-medium").installed,
            "onnx without json must not count as installed"
        );

        fs::write(tts.join("en_US-joe-medium.onnx.json"), b"{}").unwrap();
        let catalog = list_curated_piper_voices(&tts);
        assert!(
            entry(&catalog, "en_US-joe-medium").installed,
            "onnx+json pair should count as installed"
        );

        let _ = fs::remove_dir_all(&root);
    }
}
