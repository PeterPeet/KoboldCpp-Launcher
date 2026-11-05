//! Model architectures KoboldCpp can load, from its `src/llama-arch.cpp` at a release tag
//! (`tools/extract_archs.py`). Used to tell whether a GGUF file will run.

use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Deserialize)]
struct Table {
    version: Option<String>,
    archs: Vec<String>,
}

static TABLE: LazyLock<Table> = LazyLock::new(|| serde_json::from_str(include_str!("archs.json")).expect("archs.json is valid"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Support {
    Yes,
    /// Not in the list of the installed (or an older) KoboldCpp: it will not load.
    No,
    /// Not in our list, but a newer KoboldCpp is installed that may know it.
    Unknown,
}

/// KoboldCpp version the table was taken from, e.g. "v1.124.2".
pub fn table_version() -> Option<&'static str> {
    TABLE.version.as_deref()
}

/// Can KoboldCpp `installed` (e.g. "v1.124.2") load a model of architecture `arch`?
pub fn support(arch: &str, installed: Option<&str>) -> Support {
    if TABLE.archs.iter().any(|a| a.eq_ignore_ascii_case(arch)) {
        return Support::Yes;
    }
    match (installed, table_version()) {
        (Some(inst), Some(tbl)) if crate::binary::is_newer(inst, tbl) => Support::Unknown,
        // Same or older version, or unknown build: our list is what it supports.
        (Some(_), Some(_)) => Support::No,
        _ => Support::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_and_unknown_architectures() {
        assert_eq!(table_version(), Some("v1.124.2"));
        for a in ["llama", "qwen35", "gemma4", "mistral3", "lfm3", "gpt-oss", "bert"] {
            assert_eq!(support(a, Some("v1.124.2")), Support::Yes, "{a}");
        }
        assert_eq!(support("future-arch-9", Some("v1.124.2")), Support::No);
        assert_eq!(support("future-arch-9", Some("v1.130.0")), Support::Unknown, "newer KoboldCpp may know it");
        assert_eq!(support("future-arch-9", None), Support::Unknown, "custom build");
    }
}
