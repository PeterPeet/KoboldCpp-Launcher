//! Command line equivalent of a config, for "Copy as command".
//!
//! The launcher itself starts KoboldCpp with `--config <file>`, so this never drives a launch;
//! it is built from the same `Kcpps` the launch uses, so the two cannot drift apart.

use super::args::{self, ArgKind};
use super::kcpps::Kcpps;
use serde_json::Value;

/// Keys that never become flags.
const SKIP: &[&str] = &["istemplate", "port_param", "model", "showgui", "skiplauncher"];

/// Arguments (without the program name) in the upstream option order.
pub fn to_args(cfg: &Kcpps) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(m) = cfg.model() {
        out.push("--model".into());
        out.push(m);
    }
    // KoboldCpp's options in upstream order, then Esobold's extras (present only in Esobold mode).
    for spec in args::table().args.iter().chain(args::esobold_extras()) {
        let key = spec.dest.as_str();
        if key == "model_param" || SKIP.contains(&key) || args::dropped_on_save(key) {
            continue;
        }
        let Some(flag) = spec.long_flag() else { continue };
        let Some(value) = cfg.get(key) else { continue };
        if key == "port" && cfg.port() == 5001 {
            continue;
        }
        if same_value(value, &spec.default) {
            continue;
        }
        match (spec.kind, value) {
            (_, Value::Null) => {}
            (ArgKind::Flag, v) => {
                if truthy(v) {
                    out.push(flag.into());
                }
            }
            (ArgKind::List, Value::Array(items)) => {
                out.push(flag.into());
                out.extend(items.iter().map(scalar));
            }
            (ArgKind::List, v) => {
                out.push(flag.into());
                out.push(scalar(v));
            }
            (ArgKind::Optional, v) if spec.const_value.as_ref().is_some_and(|c| same_value(v, c)) => out.push(flag.into()),
            (_, Value::String(s)) if s.is_empty() => {}
            (_, v) => {
                out.push(flag.into());
                out.push(scalar(v));
            }
        }
    }
    out
}

/// Shell-style string for display: `koboldcpp --model "my model.gguf" --contextsize 8192`.
pub fn to_command(cfg: &Kcpps, program: &str) -> String {
    std::iter::once(program.to_string()).chain(to_args(cfg)).map(|a| quote(&a)).collect::<Vec<_>>().join(" ")
}

fn quote(a: &str) -> String {
    if !a.is_empty() && a.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:=,@+%".contains(c)) {
        a.to_string()
    } else {
        format!("\"{}\"", a.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Null => false,
        _ => true,
    }
}

fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => if *b { "1".into() } else { "0".into() },
        other => other.to_string(),
    }
}

/// Equality that treats 1 and 1.0 as the same and ignores int/float JSON differences.
fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same_value(p, q)),
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::kcpps::Backend;
    use serde_json::json;

    #[test]
    fn esobold_options_become_flags() {
        let mut k = Kcpps::new();
        k.set_model("m.gguf");
        k.set("jinja_tools", true.into());
        k.set("fsmaxsize", 512.into());
        assert_eq!(to_command(&k, "koboldcpp"), "koboldcpp --model m.gguf --jinja_tools --fsmaxsize 512");
    }

    #[test]
    fn builds_typical_command() {
        let mut k = Kcpps::new();
        k.set_model("/models/Qwen3 8B.gguf");
        k.set_backend(&Backend::Cuda { gpu: Some(0), lowvram: false, mmq: true });
        k.set("gpulayers", 65.into());
        k.set("tensor_split", json!([60.0, 40.0]));
        k.set("contextsize", 32768.into());
        k.set("noflashattention", true.into());
        k.set("quantkv", "q8_0".into());
        k.set("contextsize_unused_default", 1.into()); // unknown key: kept in file, no flag
        let cmd = to_command(&k, "koboldcpp");
        assert_eq!(
            cmd,
            "koboldcpp --model \"/models/Qwen3 8B.gguf\" --usecuda normal 0 --contextsize 32768 --gpulayers 65 \
             --tensor_split 60.0 40.0 --noflashattention --quantkv q8_0"
        );
    }

    #[test]
    fn defaults_and_empty_values_are_left_out() {
        let mut k = Kcpps::new();
        k.set("contextsize", 16384.into());
        k.set("launch", false.into());
        k.set("mmproj", "".into());
        k.set("port", 5001.into());
        assert!(to_args(&k).is_empty());
    }

    #[test]
    fn vulkan_autodetect_is_bare_flag() {
        let mut k = Kcpps::new();
        k.set_backend(&Backend::Vulkan { gpus: vec![] });
        assert_eq!(to_args(&k), ["--usevulkan"]);
    }
}
