//! Records what the real commands answer on this computer, for README screenshots: the browser
//! preview loads the file (`?fixture`) instead of its sample data, so the pictures show real
//! hardware, KoboldCpp templates, Hugging Face lists and the local library.
//!
//! Run: KL_CA_BUNDLE=/etc/ssl/cert.pem cargo test --test screenshot_data -- --ignored --nocapture
//! Reads the launcher's data folder (settings, models, KoboldCpp), writes nothing there; the
//! result goes to `shots-data/fixture.json` in the repository (git-ignored: local paths).

// Not on Windows: a test program that links Tauri's window code needs the Windows manifest the
// app embeds, and fails to start without it (STATUS_ENTRYPOINT_NOT_FOUND). Screenshots are
// taken on a Mac anyway.
#![cfg(not(windows))]

use kobold_launcher_lib::commands::{self as c, AppState, LocalFitInput, Machine};
use kobold_launcher_lib::{fit, hf};
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use tauri::Manager;

fn data_dir() -> PathBuf {
    std::env::var_os("KL_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| dirs::data_dir().unwrap().join("io.github.peterpeet.koboldlauncher"))
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn record_screenshot_data() {
    // A scratch data folder: copied settings, links to the real models, KoboldCpp and presets.
    let real = data_dir();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    // Same settings, but downloads stay in the real models folder (real paths, measured speeds).
    let mut settings: Value = serde_json::from_slice(&std::fs::read(real.join("settings.json")).unwrap()).unwrap();
    if settings["modelsDir"].is_null() {
        settings["modelsDir"] = json!(real.join("models"));
    }
    std::fs::write(root.join("settings.json"), serde_json::to_vec_pretty(&settings).unwrap()).unwrap();
    // Hugging Face answers are cached in shots-data/cache (seed it from the app's cache folder),
    // so repeated runs don't hit the rate limit.
    let dest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../shots-data");
    std::fs::create_dir_all(dest.join("cache")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(dest.join("cache"), root.join("cache")).unwrap();
    #[cfg(unix)]
    for d in ["bin", "presets", "runs"] {
        if real.join(d).exists() {
            std::os::unix::fs::symlink(real.join(d), root.join(d)).unwrap();
        }
    }
    let app = tauri::test::mock_app();
    app.manage(AppState::new(root.clone()));
    let st = || app.state::<AppState>();

    let mut out = Map::new();
    let mut put = |k: &str, v: Value| {
        println!("recorded {k}");
        out.insert(k.to_string(), v);
    };
    let v = |x: &dyn erased::Ser| x.json();

    let mut settings = serde_json::to_value(c::get_settings(st())).unwrap();
    // Paths of the scratch folder would be wrong in the pictures: point them back at the real one.
    let fix = |v: &mut Value| {
        let s = serde_json::to_string(v).unwrap().replace(&*root.to_string_lossy(), &real.to_string_lossy());
        *v = serde_json::from_str(&s).unwrap();
    };
    fix(&mut settings);
    put("get_settings", settings);
    let mut paths = serde_json::to_value(c::app_paths(st())).unwrap();
    fix(&mut paths);
    put("app_paths", paths);

    let hw = c::hardware_info(st(), None).await.unwrap();
    put("hardware_info", v(&hw));
    put("binary_status", v(&c::binary_status(st()).await.unwrap()));
    put("check_update", v(&c::check_update(st()).await.unwrap()));
    put("launcher_update", v(&c::launcher_update(st()).await.unwrap()));
    put("list_presets", v(&c::list_presets(st())));
    put("last_config", v(&c::last_config(st())));
    put("server_status", json!({ "state": "stopped" }));

    // The machine the UI computes with every GPU ticked (state.ts `machine()`).
    let vram: u64 = hw.gpus.iter().map(|g| g.vram).sum();
    let gpu_bw = if vram == 0 {
        None
    } else {
        let bws: Option<Vec<f64>> = hw.gpus.iter().map(|g| g.bandwidth.or(if g.discrete { None } else { hw.ram_bandwidth })).collect();
        bws.map(|b| 1.0 / hw.gpus.iter().zip(b).map(|(g, b)| g.vram as f64 / vram as f64 / b).sum::<f64>())
    };
    let machine = || Machine {
        vram,
        ram: hw.ram_total,
        unified: hw.unified_memory && vram > 0,
        context: 16384,
        speed: fit::SpeedHw { gpu_bandwidth: gpu_bw, cpu_bandwidth: hw.ram_bandwidth, apple: hw.unified_memory },
    };

    let templates = c::templates(st()).await.unwrap();
    let mut fits = Map::new();
    for t in &templates {
        let (size, ctx) = (t.size.unwrap_or(0), t.context.unwrap_or(8192) as u32);
        let f = c::fits(vec![(size, ctx)], machine().vram, machine().ram, machine().unified);
        fits.insert(format!("{size}:{ctx}"), v(&f[0]));
    }
    put("templates", v(&templates));
    put("fits", Value::Object(fits));

    let families = c::model_families();
    put("model_families", v(&families));
    let opts = || hf::RankOptions { official_only: false, show_uncensored: true, sort: hf::Sort::Downloads };
    let previous: Value = std::fs::read(dest.join("fixture.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let mut popular = Map::new();
    let keys: Vec<Option<String>> = std::iter::once(None).chain(families.iter().map(|f| Some(f.0.clone()))).collect();
    for family in keys {
        let key = family.clone().unwrap_or_default();
        let r = v(&c::popular_models(st(), opts(), machine(), family).await.unwrap());
        // Rate-limited (errors): keep the last good recording of this list.
        let failed = r["errors"].as_array().is_some_and(|e| !e.is_empty());
        let old = &previous["popular_models"][&key];
        let keep = failed && old.is_object() && old["errors"].as_array().is_none_or(|e| e.is_empty());
        if failed {
            println!("popular {key}: {} errors{}", r["errors"].as_array().unwrap().len(), if keep { ", keeping the last recording" } else { "" });
        }
        popular.insert(key.clone(), if keep { previous["popular_models"][&key].clone() } else { r });
    }
    put("popular_models", Value::Object(popular));

    // Search isn't cached; when Hugging Face rate-limits (429), keep the last recorded answer.
    let mut search = Map::new();
    for q in ["gemma", "qwen", "mistral"] {
        match c::search_models(st(), q.into(), machine()).await {
            Ok(r) => search.insert(q.into(), v(&r)),
            Err(e) => {
                println!("search {q}: {e}, keeping the last recording");
                search.insert(q.into(), previous["search_models"][q].clone())
            }
        };
    }
    put("search_models", Value::Object(search));

    let lib = c::library_scan(st()).await.unwrap();
    let inputs: Vec<LocalFitInput> = lib
        .models
        .iter()
        .map(|m| LocalFitInput {
            size: m.size,
            layers: m.layers,
            head_count_kv: m.head_count_kv,
            kv_len: m.kv_len,
            mmproj: m.mmproj.as_ref().and_then(|p| std::fs::metadata(p).ok()).map(|x| x.len()),
            path: Some(m.path.to_string_lossy().into_owned()),
            name: Some(format!("{} {}", m.name, m.file)),
        })
        .collect();
    let paths: Vec<String> = lib.models.iter().map(|m| m.path.to_string_lossy().into_owned()).collect();
    let fitted = c::fit_local(st(), inputs, machine());
    let by_path: Map<String, Value> = paths.into_iter().zip(fitted.iter().map(|f| v(f))).collect();
    let mut lib_v = v(&lib);
    fix(&mut lib_v);
    put("library_scan", lib_v);
    put("fit_local", Value::Object(by_path));

    std::fs::create_dir_all(&dest).unwrap();
    std::fs::write(dest.join("fixture.json"), serde_json::to_string_pretty(&Value::Object(out)).unwrap()).unwrap();
    println!("wrote {}", dest.join("fixture.json").display());
}

/// `serde_json::to_value` for any command result.
mod erased {
    pub trait Ser {
        fn json(&self) -> serde_json::Value;
    }
    impl<T: serde::Serialize> Ser for T {
        fn json(&self) -> serde_json::Value {
            serde_json::to_value(self).unwrap()
        }
    }
}
