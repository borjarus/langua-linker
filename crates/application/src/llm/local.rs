//! On-device association models (ADR-23): the catalog of models the app may download.
//!
//! The catalog is `content/models/catalog.json`, compiled in. Each entry is the manifest the
//! fine-tuning notebook writes next to the GGUF file it publishes on GitHub Releases; the app
//! downloads only listed files and only keeps one whose size and SHA-256 match. An empty catalog
//! hides the on-device provider.

use std::sync::LazyLock;

use serde::Deserialize;

/// The compiled-in catalog file.
const CATALOG_JSON: &str = include_str!("../../../../content/models/catalog.json");

/// Hosts a model URL may point to (GitHub Releases and the storage it redirects to).
pub const MODEL_HOSTS: [&str; 3] = [
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

/// One downloadable model.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModel {
    /// Stable id, stored as the settings' model (`bielik-1.5b-associations-v1`).
    pub id: String,
    /// Shown in the settings.
    pub name: String,
    /// File name on disk and in the release (`<id>.Q4_K_M.gguf`).
    pub file: String,
    /// `https://github.com/<owner>/<repo>/releases/download/<tag>/<file>`.
    pub url: String,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: String,
    pub bytes: u64,
    /// Context window the app opens the model with.
    #[serde(default = "default_context")]
    pub context_tokens: u32,
    /// Prompt versions the model was trained on (informative).
    #[serde(default)]
    pub prompt_versions: Vec<String>,
}

fn default_context() -> u32 {
    2_048
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    models: Vec<LocalModel>,
}

static CATALOG: LazyLock<Vec<LocalModel>> = LazyLock::new(|| parse(CATALOG_JSON));

/// Valid entries of a catalog file; an entry that fails [`LocalModel::is_valid`] is dropped, so
/// a bad edit can never make the app download from elsewhere.
pub fn parse(json: &str) -> Vec<LocalModel> {
    serde_json::from_str::<Catalog>(json)
        .map(|catalog| {
            catalog
                .models
                .into_iter()
                .filter(LocalModel::is_valid)
                .collect()
        })
        .unwrap_or_default()
}

/// The compiled-in catalog.
pub fn catalog() -> &'static [LocalModel] {
    &CATALOG
}

/// A model of the compiled-in catalog.
pub fn find(id: &str) -> Option<&'static LocalModel> {
    catalog().iter().find(|model| model.id == id)
}

impl LocalModel {
    /// A plain file name, an HTTPS URL on a release host, a 64-digit hex digest and a size.
    pub fn is_valid(&self) -> bool {
        let plain = |value: &str| {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
                && !value.starts_with('.')
        };
        let host_ok = self
            .url
            .strip_prefix("https://")
            .and_then(|rest| rest.split('/').next())
            .is_some_and(|host| MODEL_HOSTS.contains(&host));
        plain(&self.id)
            && plain(&self.file)
            && self.file.ends_with(".gguf")
            && host_ok
            && self.sha256.len() == 64
            && self
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            && self.bytes > 0
            && !self.name.trim().is_empty()
            && (256..=32_768).contains(&self.context_tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str = r#"{
        "id": "bielik-1.5b-associations-v1",
        "name": "Bielik 1.5B (skojarzenia)",
        "file": "bielik-1.5b-associations-v1.Q4_K_M.gguf",
        "url": "https://github.com/borjarus/langua-linker/releases/download/model-bielik-associations-v1/bielik-1.5b-associations-v1.Q4_K_M.gguf",
        "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "bytes": 1000000000,
        "base": "speakleash/Bielik-1.5B-v3.0-Instruct",
        "trainedAt": "2026-11-20"
    }"#;

    fn with(change: impl FnOnce(&mut serde_json::Value)) -> Vec<LocalModel> {
        let mut entry: serde_json::Value = serde_json::from_str(ENTRY).unwrap();
        change(&mut entry);
        parse(&serde_json::json!({ "schemaVersion": 1, "models": [entry] }).to_string())
    }

    #[test]
    fn the_notebook_manifest_is_a_catalog_entry() {
        let models = with(|_| {});
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].context_tokens, 2_048);
        assert_eq!(models[0].bytes, 1_000_000_000);
    }

    #[test]
    fn entries_outside_the_rules_are_dropped() {
        for change in [
            |e: &mut serde_json::Value| e["url"] = "http://github.com/x/y.gguf".into(),
            |e: &mut serde_json::Value| e["url"] = "https://example.com/model.gguf".into(),
            |e: &mut serde_json::Value| e["file"] = "../model.gguf".into(),
            |e: &mut serde_json::Value| e["file"] = "model.bin".into(),
            |e: &mut serde_json::Value| e["sha256"] = "XYZ".into(),
            |e: &mut serde_json::Value| e["bytes"] = 0.into(),
        ] {
            assert!(with(change).is_empty());
        }
        assert!(parse("not json").is_empty());
    }

    #[test]
    fn the_compiled_catalog_parses() {
        // Empty until the first model is released; every listed entry must be valid.
        let raw: serde_json::Value = serde_json::from_str(CATALOG_JSON).unwrap();
        assert_eq!(
            raw["models"].as_array().map_or(0, Vec::len),
            catalog().len(),
            "an entry of content/models/catalog.json is invalid"
        );
    }
}
