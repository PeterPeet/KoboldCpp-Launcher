//! `.kcpps` settings files and `.kcppt` templates.
//!
//! KoboldCpp stores its parsed arguments as a flat JSON object (`vars(args)`). When it loads a
//! file it applies its own migrations and fills missing keys with argparse defaults, so we keep
//! files **sparse**: we preserve every key we read (including ones we don't know) and only add
//! the keys the user changes.

use super::args;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not read or write the settings file: {0}")]
    Io(#[from] std::io::Error),
    #[error("the settings file is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the settings file must contain a JSON object")]
    NotObject,
}

/// Which GPU backend the config asks for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Backend {
    /// No backend flag: KoboldCpp picks one itself (and always uses Metal on macOS).
    Auto,
    /// `--usecuda` (also covers ROCm/hipBLAS builds). `gpu`: one device, or all when `None`.
    Cuda { gpu: Option<u32>, lowvram: bool, mmq: bool },
    /// `--usevulkan`; empty list = autodetect.
    Vulkan { gpus: Vec<u32> },
    /// `--usecpu`
    Cpu,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Kcpps {
    map: Map<String, Value>,
}

impl Kcpps {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_map(map: Map<String, Value>) -> Self {
        Self { map }
    }

    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        match serde_json::from_str::<Value>(text)? {
            Value::Object(map) => Ok(Self { map }),
            _ => Err(ConfigError::NotObject),
        }
    }

    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        // KoboldCpp reads with errors='ignore'; be as lenient with stray bytes.
        let bytes = std::fs::read(path)?;
        Self::parse(&String::from_utf8_lossy(&bytes))
    }

    pub fn map(&self) -> &Map<String, Value> {
        &self.map
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.map.get(key)
    }

    /// The value KoboldCpp will use: the stored one, else the argparse default.
    pub fn effective(&self, key: &str) -> Value {
        self.map.get(key).cloned().unwrap_or_else(|| args::default_of(key))
    }

    pub fn set(&mut self, key: &str, value: Value) {
        self.map.insert(key.to_string(), value);
    }

    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.map.shift_remove(key)
    }

    pub fn get_str(&self, key: &str) -> Option<String> {
        match self.effective(key) {
            Value::String(s) if !s.is_empty() => Some(s),
            _ => None,
        }
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        match self.effective(key) {
            Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
            Value::String(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn get_bool(&self, key: &str) -> bool {
        match self.effective(key) {
            Value::Bool(b) => b,
            Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
            _ => false,
        }
    }

    pub fn is_template(&self) -> bool {
        self.get_bool("istemplate")
    }

    /// Main text model (path or URL). Older files use `model` (string or list).
    pub fn model(&self) -> Option<String> {
        if let Some(m) = self.get_str("model_param") {
            return Some(m);
        }
        match self.map.get("model") {
            Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
            Some(Value::Array(a)) => a.first().and_then(Value::as_str).map(str::to_string),
            _ => None,
        }
    }

    pub fn set_model(&mut self, path: &str) {
        self.set("model_param", Value::String(path.to_string()));
        self.remove("model");
    }

    pub fn port(&self) -> u16 {
        let p = self.get_i64("port").unwrap_or(5001);
        let pp = self.get_i64("port_param").unwrap_or(5001);
        // KoboldCpp: a non-default positional port wins.
        let chosen = if pp != 5001 { pp } else { p };
        u16::try_from(chosen).unwrap_or(5001)
    }

    pub fn set_port(&mut self, port: u16) {
        self.set("port", port.into());
        self.set("port_param", port.into());
    }

    pub fn backend(&self) -> Backend {
        if let Some(Value::Array(list)) = self.map.get("usecuda").or_else(|| self.map.get("usecublas")) {
            let items: Vec<String> = list.iter().map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).collect();
            let gpu = items.iter().find_map(|s| s.parse::<u32>().ok());
            return Backend::Cuda {
                gpu,
                lowvram: items.iter().any(|s| s == "lowvram") || self.get_bool("lowvram"),
                mmq: !(items.iter().any(|s| s == "nommq") || self.get_bool("nommq")),
            };
        }
        if let Some(Value::Array(list)) = self.map.get("usevulkan") {
            return Backend::Vulkan { gpus: list.iter().filter_map(|v| v.as_u64().map(|n| n as u32)).collect() };
        }
        if self.get_bool("usecpu") || self.get_bool("noblas") {
            return Backend::Cpu;
        }
        Backend::Auto
    }

    pub fn set_backend(&mut self, backend: &Backend) {
        for k in ["usecuda", "usecublas", "usevulkan"] {
            self.set(k, Value::Null);
        }
        self.remove("usecublas");
        self.set("usecpu", false.into());
        match backend {
            Backend::Auto => {}
            Backend::Cuda { gpu, lowvram, mmq } => {
                let mut list = vec![Value::from("normal")];
                if let Some(g) = gpu {
                    list.push(Value::from(g.to_string()));
                }
                self.set("usecuda", Value::Array(list));
                self.set("lowvram", (*lowvram).into());
                self.set("nommq", (!mmq).into());
            }
            Backend::Vulkan { gpus } => {
                self.set("usevulkan", Value::Array(gpus.iter().map(|&g| g.into()).collect()));
            }
            Backend::Cpu => self.set("usecpu", true.into()),
        }
    }

    /// Bring legacy keys in line with what current KoboldCpp reads, so the UI shows the right
    /// values (KoboldCpp does the same on load: `convert_invalid_args`). Only fills keys that
    /// are missing; never overwrites current ones.
    pub fn migrate(&mut self) {
        let fill = |m: &mut Map<String, Value>, key: &str, v: Value| {
            if !m.contains_key(key) {
                m.insert(key.to_string(), v);
            }
        };
        if let Some(v) = self.map.get("usecublas").cloned().filter(|v| !v.is_null()) {
            fill(&mut self.map, "usecuda", v);
        }
        if let Some(Value::Array(list)) = self.map.get("usecuda").cloned() {
            let has = |w: &str| list.iter().any(|v| v.as_str() == Some(w));
            if has("lowvram") {
                self.set("lowvram", true.into());
            }
            if has("nommq") {
                self.set("nommq", true.into());
            }
            if has("rowsplit") {
                self.set("splitmode", "row".into());
            }
        }
        if let Some(v) = self.map.get("blasbatchsize").cloned().filter(|v| !v.is_null()) {
            fill(&mut self.map, "batchsize", v);
        }
        if self.get_str("model_param").is_none()
            && let Some(m) = self.model()
        {
            self.set("model_param", m.into());
        }
        if let Some(fa) = self.map.get("flashattention").and_then(Value::as_bool) {
            fill(&mut self.map, "noflashattention", (!fa).into());
        }
        if let Some(swa) = self.map.get("useswa").and_then(Value::as_bool) {
            fill(&mut self.map, "noswa", (!swa).into());
        }
        if self.map.get("noblas").and_then(Value::as_bool) == Some(true) {
            self.set("usecpu", true.into());
        }
        let pp = self.map.get("port_param").and_then(Value::as_i64);
        if let Some(pp) = pp.filter(|&p| p != 5001) {
            self.set("port", pp.into());
        }
    }

    /// Turn machine-specific settings back into "decide on this PC" values
    /// (`convert_args_to_template`).
    pub fn to_template(&self) -> Kcpps {
        let mut t = self.clone();
        let resets: &[(&str, Value)] = &[
            ("istemplate", true.into()), ("gpulayers", (-1).into()), ("threads", (-1).into()),
            ("hordekey", "".into()), ("hordeworkername", "".into()), ("sdthreads", 0.into()),
            ("maingpu", (-1).into()), ("sdmaingpu", (-1).into()), ("password", Value::Null),
            ("adminpassword", Value::Null), ("usemmap", false.into()), ("usemlock", false.into()),
            ("usedirectio", false.into()), ("debugmode", 0.into()), ("ssl", Value::Null),
            ("usecuda", Value::Null), ("usevulkan", Value::Null), ("usecpu", Value::Null),
            ("tensor_split", Value::Null), ("draftgpusplit", Value::Null), ("config", Value::Null),
            ("ttsthreads", 0.into()), ("nommq", false.into()), ("splitmode", "layer".into()),
        ];
        for (k, v) in resets {
            t.set(k, v.clone());
        }
        t
    }

    /// Every http(s) URL in the file with its key: what a template will download at launch.
    pub fn urls(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (k, v) in &self.map {
            match v {
                Value::String(s) if is_url(s) => out.push((k.clone(), s.clone())),
                Value::Array(a) => out.extend(a.iter().filter_map(Value::as_str).filter(|s| is_url(s)).map(|s| (k.clone(), s.to_string()))),
                _ => {}
            }
        }
        out
    }

    /// JSON text as KoboldCpp writes it (indent 2), minus keys it never saves.
    pub fn to_json_string(&self) -> String {
        let mut out = Map::new();
        for (k, v) in &self.map {
            if args::dropped_on_save(k) {
                continue;
            }
            let v = match (k.as_str(), v) {
                // gendefaults is stored as an object when it parses
                ("gendefaults", Value::String(s)) => match serde_json::from_str::<Value>(s) {
                    Ok(obj @ Value::Object(_)) => obj,
                    _ => v.clone(),
                },
                _ => v.clone(),
            };
            out.insert(k.clone(), v);
        }
        serde_json::to_string_pretty(&Value::Object(out)).expect("map serializes")
    }

    /// Write atomically (temp file + rename) so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let tmp = path.with_extension("tmp-write");
        std::fs::write(&tmp, self.to_json_string())?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

pub fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unknown_keys_round_trip() {
        let k = Kcpps::parse(r#"{"model_param":"a.gguf","futureoption":{"x":[1,2]},"contextsize":8192}"#).unwrap();
        let back = Kcpps::parse(&k.to_json_string()).unwrap();
        assert_eq!(back.get("futureoption"), Some(&json!({"x":[1,2]})));
        let keys: Vec<_> = back.map().keys().cloned().collect();
        assert_eq!(keys, ["model_param", "futureoption", "contextsize"]);
    }

    #[test]
    fn effective_falls_back_to_defaults() {
        let k = Kcpps::new();
        assert_eq!(k.get_i64("contextsize"), Some(16384));
        assert_eq!(k.get_i64("gpulayers"), Some(-1));
        assert_eq!(k.port(), 5001);
        assert_eq!(k.backend(), Backend::Auto);
    }

    #[test]
    fn migrates_legacy_keys() {
        let mut k = Kcpps::parse(r#"{"model":["x.gguf"],"usecublas":["normal","1","rowsplit"],"blasbatchsize":1024,"flashattention":true,"port_param":5002}"#).unwrap();
        k.migrate();
        assert_eq!(k.model().as_deref(), Some("x.gguf"));
        assert_eq!(k.get_str("model_param").as_deref(), Some("x.gguf"));
        assert_eq!(k.get_i64("batchsize"), Some(1024));
        assert!(!k.get_bool("noflashattention"));
        assert_eq!(k.get_str("splitmode").as_deref(), Some("row"));
        assert_eq!(k.port(), 5002);
        assert_eq!(k.backend(), Backend::Cuda { gpu: Some(1), lowvram: false, mmq: true });
    }

    #[test]
    fn backend_round_trip() {
        let mut k = Kcpps::new();
        for b in [
            Backend::Cuda { gpu: None, lowvram: false, mmq: true },
            Backend::Cuda { gpu: Some(1), lowvram: true, mmq: false },
            Backend::Vulkan { gpus: vec![] },
            Backend::Vulkan { gpus: vec![0, 1] },
            Backend::Cpu,
            Backend::Auto,
        ] {
            k.set_backend(&b);
            assert_eq!(k.backend(), b);
        }
    }

    #[test]
    fn template_resets_machine_settings_and_drops_unsaved_keys() {
        let mut k = Kcpps::new();
        k.set("gpulayers", 40.into());
        k.set("usecuda", json!(["normal", "0"]));
        k.set("config", json!(["x.kcpps"]));
        let t = k.to_template();
        assert!(t.is_template());
        assert_eq!(t.get_i64("gpulayers"), Some(-1));
        assert_eq!(t.backend(), Backend::Auto);
        assert!(!t.to_json_string().contains("\"config\""));
    }
}
