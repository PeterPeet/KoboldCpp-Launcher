//! KoboldCpp's launch options, extracted from upstream `koboldcpp.py` by `tools/extract_args.py`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArgKind {
    /// `action='store_true'`
    Flag,
    /// Single value.
    Value,
    /// `nargs='*'`, `'+'` or a number.
    List,
    /// `nargs='?'`: flag alone means `const`.
    Optional,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgSpec {
    pub dest: String,
    pub flags: Vec<String>,
    pub group: String,
    pub exclusive: Option<String>,
    pub kind: ArgKind,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default)]
    pub default: Value,
    #[serde(default, rename = "const")]
    pub const_value: Option<Value>,
    #[serde(default)]
    pub choices: Option<Vec<Value>>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub dynamic: bool,
    #[serde(default)]
    pub help: Option<String>,
}

impl ArgSpec {
    /// The long option name used on the command line, e.g. `--contextsize`.
    /// `None` for positionals.
    pub fn long_flag(&self) -> Option<&str> {
        self.flags
            .iter()
            .find(|f| f.starts_with("--"))
            .or_else(|| self.flags.iter().find(|f| f.starts_with('-')))
            .map(String::as_str)
    }

    pub fn is_positional(&self) -> bool {
        self.long_flag().is_none()
    }
}

#[derive(Debug, Deserialize)]
pub struct ArgTable {
    pub version: Option<String>,
    pub args: Vec<ArgSpec>,
    #[serde(skip)]
    index: HashMap<String, usize>,
}

impl ArgTable {
    pub fn get(&self, dest: &str) -> Option<&ArgSpec> {
        self.index.get(dest).map(|&i| &self.args[i])
    }
}

static TABLE: LazyLock<ArgTable> = LazyLock::new(|| {
    let mut t: ArgTable = serde_json::from_str(include_str!("args.json")).expect("args.json is valid");
    t.index = t.args.iter().enumerate().map(|(i, a)| (a.dest.clone(), i)).collect();
    t
});

pub fn table() -> &'static ArgTable {
    &TABLE
}

pub fn spec(dest: &str) -> Option<&'static ArgSpec> {
    TABLE.get(dest)
}

/// Options Esobold has and KoboldCpp doesn't (from Esobold's koboldcpp.py, `args-esobold.json`,
/// refreshed with `tools/extract_args.py` like the main table).
static ESOBOLD_EXTRAS: LazyLock<Vec<ArgSpec>> = LazyLock::new(|| {
    let t: ArgTable = serde_json::from_str(include_str!("args-esobold.json")).expect("args-esobold.json is valid");
    t.args.into_iter().filter(|a| TABLE.get(&a.dest).is_none()).collect()
});

/// Groups of Esobold extras that only work in admin or autoswap mode, which the launcher blocks
/// (`safety::NEVER`): not offered.
const ESOBOLD_ADMIN_GROUPS: &[&str] = &["Administration Commands", "Autoswap Options (Admin Mode)"];

pub fn esobold_extras() -> &'static [ArgSpec] {
    &ESOBOLD_EXTRAS
}

pub fn esobold_spec(dest: &str) -> Option<&'static ArgSpec> {
    ESOBOLD_EXTRAS.iter().find(|a| a.dest == dest)
}

/// The Esobold extras Advanced setup offers in Esobold mode.
pub fn esobold_offered() -> Vec<ArgSpec> {
    ESOBOLD_EXTRAS.iter().filter(|a| !a.hidden && !ESOBOLD_ADMIN_GROUPS.contains(&a.group.as_str())).cloned().collect()
}

/// Default value for a key, or `Null` if unknown or computed at runtime.
pub fn default_of(dest: &str) -> Value {
    spec(dest).map(|s| s.default.clone()).unwrap_or(Value::Null)
}

/// Keys KoboldCpp never writes to a `.kcpps` (`save_config_dict`).
pub const DO_NOT_SAVE: &[&str] = &[
    "agent_api_key", "agent_base_url", "allow_config_onready", "analyze", "config", "exportconfig",
    "exporttemplate", "run_bundled_agent", "testmemory", "unpack", "version",
];

/// KoboldCpp drops this key when saving a config (never saved, or a legacy name). A legacy
/// KoboldCpp name that Esobold uses as a real option (`jinja_tools`) is kept.
pub fn dropped_on_save(key: &str) -> bool {
    DO_NOT_SAVE.contains(&key) || (DEPRECATED.contains(&key) && esobold_spec(key).is_none())
}

/// Legacy keys KoboldCpp still reads but drops on save (`deprecated_keys`).
pub const DEPRECATED: &[&str] = &[
    "hordeconfig", "sdconfig", "noblas", "nommap", "pipelineparallel", "nopipelineparallel", "sdnotile",
    "sdt5xxl", "forceversion", "sdgendefaults", "flashattention", "useswa", "jinja_tools", "multiuser",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_loads_core_options() {
        let t = table();
        assert!(t.args.len() > 150);
        let ctx = spec("contextsize").unwrap();
        assert_eq!(ctx.long_flag(), Some("--contextsize"));
        assert_eq!(ctx.default, serde_json::json!(16384));
        assert_eq!(spec("gpulayers").unwrap().kind, ArgKind::Optional);
        assert_eq!(spec("usecuda").unwrap().kind, ArgKind::List);
        assert_eq!(spec("noflashattention").unwrap().kind, ArgKind::Flag);
        assert!(spec("model_param").unwrap().is_positional());
    }
}
