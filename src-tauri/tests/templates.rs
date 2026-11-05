//! Real KoboldCpp templates from hf.co/koboldcpp (fetched into tests/fixtures).

use kobold_launcher_lib::config::{cli, Backend, Kcpps};
use std::path::PathBuf;

fn fixture(name: &str) -> Kcpps {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    Kcpps::load(&p).unwrap()
}

#[test]
fn midspec_chatbot_reads_as_expected() {
    let k = fixture("MidSpec-Chatbot.kcppt");
    assert!(k.is_template());
    assert_eq!(k.get_i64("contextsize"), Some(24576));
    assert_eq!(k.backend(), Backend::Auto);
    assert!(k.model().unwrap().ends_with("Qwen3VL-8B-Instruct-Q4_K_M.gguf"));
    let keys: Vec<String> = k.urls().into_iter().map(|(k, _)| k).collect();
    for want in ["model_param", "mmproj", "whispermodel", "ttsmodel", "embeddingsmodel"] {
        assert!(keys.iter().any(|k| k == want), "missing url key {want}");
    }
}

#[test]
fn image_template_urls_include_sd_parts() {
    let k = fixture("flux2-klein-4b.kcppt");
    let keys: Vec<String> = k.urls().into_iter().map(|(k, _)| k).collect();
    assert!(keys.contains(&"sdmodel".to_string()));
    assert!(keys.contains(&"sdvae".to_string()));
}

#[test]
fn templates_round_trip_without_losing_keys() {
    for name in ["MidSpec-Chatbot.kcppt", "LowSpec-Chatbot.kcppt", "HighSpec-CodingAgent.kcppt", "flux2-klein-4b.kcppt"] {
        let k = fixture(name);
        let back = Kcpps::parse(&k.to_json_string()).unwrap();
        // Only keys KoboldCpp itself drops on save may disappear.
        use kobold_launcher_lib::config::args::{DEPRECATED, DO_NOT_SAVE};
        for key in k.map().keys() {
            if back.get(key).is_none() {
                let k = key.as_str();
                assert!(DEPRECATED.contains(&k) || DO_NOT_SAVE.contains(&k), "{name}: lost {key}");
            }
        }
    }
}

#[test]
fn template_command_line_is_short_and_has_model() {
    let k = fixture("MidSpec-Chatbot.kcppt");
    let args = cli::to_args(&k);
    assert_eq!(args[0], "--model");
    assert!(args.contains(&"--contextsize".to_string()));
    assert!(args.contains(&"--mmproj".to_string()));
    assert!(!args.contains(&"--istemplate".to_string()));
    assert!(args.len() < 60, "too many flags: {args:?}");
}

#[test]
fn save_and_load_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.kcpps");
    let mut k = fixture("LowSpec-Chatbot.kcppt");
    k.set("istemplate", false.into());
    k.set("gpulayers", 20.into());
    k.save(&path).unwrap();
    let back = Kcpps::load(&path).unwrap();
    assert_eq!(back.get_i64("gpulayers"), Some(20));
    assert!(!back.is_template());
}
