//! Will a model fit, how many layers go on the GPU, which quant to pick.
//!
//! `estimate_layers` follows KoboldCpp's own launcher estimate (`autoset_gpu_layers`) so our
//! numbers match what users see in KoboldCpp. At launch we still pass `--gpulayers -1` (autofit)
//! unless the user set a number, so KoboldCpp makes the final call.

use serde::{Deserialize, Serialize};

const MIB: f64 = 1024.0 * 1024.0;
const GIB: f64 = 1024.0 * MIB;

/// Sizes in bytes of the extra models loaded next to the text model.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Extras {
    pub image: u64,
    pub whisper: u64,
    pub mmproj: u64,
    pub draft: u64,
    pub tts: u64,
    pub embeddings: u64,
    pub music_llm: u64,
    pub music_dit: u64,
    pub music_vae: u64,
}

/// What we know about the text model.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModelShape {
    /// All parts together.
    pub size: u64,
    pub layers: Option<u32>,
    pub head_count_kv: Option<u32>,
    /// max(key_length, value_length)
    pub kv_len: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub context: u32,
    pub batch: i32,
    /// `--sdquant` level 0–2.
    pub image_quant: u8,
    pub music_lowvram: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { context: 16384, batch: 512, image_quant: 0, music_lowvram: false }
    }
}

/// GPU memory the extra models need, as KoboldCpp estimates it.
pub fn extras_overhead(x: &Extras, s: &Settings) -> f64 {
    let big = |b: u64| b as f64 > 10.0 * MIB;
    let f = |b: u64| b as f64;
    let mut oh = 0.0;
    if f(x.image) > 5.0 * GIB {
        oh += GIB * (9.0 - s.image_quant as f64 * 1.5);
    } else if f(x.image) > 512.0 * MIB {
        oh += GIB * (4.25 - s.image_quant as f64 * 0.5);
    }
    if big(x.whisper) {
        oh += (350.0 * MIB).max(f(x.whisper) * 1.5);
    }
    if big(x.mmproj) {
        oh += (350.0 * MIB).max(f(x.mmproj) * 1.5);
    }
    if big(x.draft) {
        oh += f(x.draft) * 1.6 + 150.0 * MIB;
    }
    if big(x.tts) {
        oh += if f(x.tts) < GIB { (600.0 * MIB).max(f(x.tts) * 3.0) } else { (600.0 * MIB).max(150.0 * MIB + f(x.tts) * 1.3) };
    }
    if big(x.embeddings) {
        oh += (350.0 * MIB).max(f(x.embeddings) * 1.5);
    }
    let m = |b: u64, add: f64| if big(b) { f(b) * 1.1 + add } else { 0.0 };
    let (m1, m2, m3) = (m(x.music_llm, 400.0 * MIB), m(x.music_dit, 400.0 * MIB), m(x.music_vae, 0.0));
    oh += if s.music_lowvram { m1.max(m2).max(m3) } else { m1 + m2 + m3 };
    oh
}

fn context_multiplier(ctx: u32) -> f64 {
    let cs = ctx as f64;
    if ctx >= 8192 { cs / 4096.0 } else if ctx > 4096 { 1.8 } else if ctx > 2048 { 1.2 } else { 1.0 }
}

/// Layers that fit on the GPU (KoboldCpp's estimate). `layers + 1` means everything fits.
/// `vram_used` is memory other programs already use on that GPU.
pub fn estimate_layers(model: &ModelShape, extras: &Extras, s: &Settings, vram: u64, vram_used: u64) -> u32 {
    let fsize = model.size as f64;
    if fsize <= 10.0 * MIB || vram == 0 {
        return 0;
    }
    let used = vram_used as f64;
    let reserved = (1.25 * GIB).max(0.5 * GIB + used);
    let mem = (vram as f64 - extras_overhead(extras, s)).max(0.0);
    let cs = s.context as f64;
    let csmul = context_multiplier(s.context);
    let bbs = s.batch.max(1) as f64;
    let limit = match model.layers.filter(|&l| l > 0) {
        None => ((mem - used) / (fsize * csmul * 0.052)).clamp(0.0, 200.0) as u32,
        Some(layers) => {
            let l = layers as f64;
            let kvlen = model.kv_len.filter(|&k| k > 0).unwrap_or(128) as f64;
            let mut ratio = (mem - used) / (fsize * csmul * 1.6 * if bbs <= 512.0 { 1.0 } else { 1.2 });
            if let Some(heads) = model.head_count_kv.filter(|&h| h > 0) {
                let mem1 = l * if bbs <= 512.0 { 4.0 } else { bbs / 128.0 } * kvlen * cs * 4.0 * 1.45;
                let mem2 = l * heads as f64 * kvlen * cs * 4.0 * 1.15;
                ratio = ratio.max((mem - reserved - mem1) / (fsize + mem2));
            }
            ((ratio * l).max(0.0) as u32).min(layers + 1)
        }
    };
    if limit <= 2 { 0 } else { limit }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    /// Everything on the GPU.
    Full,
    /// Some layers on the CPU: works, slower.
    Partial,
    /// No usable GPU, runs on the CPU.
    Cpu,
    /// Too big for this PC.
    None,
}

/// Rough memory a model needs besides its weights: KV cache and compute buffers.
/// Used when we only know file sizes (Hugging Face listings), not the layer layout.
pub fn context_overhead(size: u64, context: u32) -> u64 {
    // ~0.5 GB base plus a share that grows with context; matches the shape of KoboldCpp's csmul.
    let per_4k = (size as f64 * 0.03).clamp(0.12 * GIB, 0.6 * GIB);
    (0.5 * GIB + per_4k * (context as f64 / 4096.0)) as u64
}

/// Fit from sizes only. `vram`: memory of the chosen GPUs (0 = CPU only); `unified`: Apple Silicon.
pub fn fit_by_size(size: u64, context: u32, vram: u64, ram: u64, unified: bool) -> Fit {
    let need = size + context_overhead(size, context);
    if unified {
        return if need <= vram { Fit::Full } else { Fit::None };
    }
    if vram == 0 {
        return if (size as f64) <= ram as f64 * 0.6 { Fit::Cpu } else { Fit::None };
    }
    if need <= vram {
        Fit::Full
    } else if (size as f64) <= vram as f64 + ram as f64 * 0.5 {
        Fit::Partial
    } else {
        Fit::None
    }
}

/// Fit from a known model layout (local files), using KoboldCpp's layer estimate.
pub fn fit_by_layers(model: &ModelShape, extras: &Extras, s: &Settings, vram: u64, vram_used: u64, ram: u64, unified: bool) -> (Fit, u32) {
    let Some(layers) = model.layers else {
        return (fit_by_size(model.size, s.context, vram, ram, unified), 0);
    };
    if unified {
        let f = fit_by_size(model.size + extras_overhead(extras, s) as u64, s.context, vram, ram, true);
        return (f, if f == Fit::Full { layers + 1 } else { 0 });
    }
    if vram == 0 {
        return (fit_by_size(model.size, s.context, 0, ram, false), 0);
    }
    let n = estimate_layers(model, extras, s, vram, vram_used);
    let fit = if n > layers {
        Fit::Full
    } else if (model.size as f64) <= vram as f64 + ram as f64 * 0.5 {
        Fit::Partial
    } else {
        Fit::None
    };
    (fit, n)
}

/// Memory a model needs while running (bytes): weights, extra models, KV cache and compute
/// buffers. With the GGUF layout the KV cache is exact for f16 (2 × layers × KV heads × head size ×
/// context × 2 bytes); without it the size-based estimate is used.
pub fn runtime_memory(model: &ModelShape, extras: &Extras, s: &Settings) -> u64 {
    let base = model.size as f64 + extras_overhead(extras, s);
    let cache = match (model.layers, model.head_count_kv, model.kv_len) {
        (Some(l), Some(h), Some(k)) if l > 0 && h > 0 && k > 0 => {
            let kv = 2.0 * l as f64 * h as f64 * k as f64 * s.context as f64 * 2.0;
            // compute buffers grow with batch size; ~0.3 GB + 5 % of the weights is typical
            kv + 0.3 * GIB + model.size as f64 * 0.05
        }
        _ => context_overhead(model.size, s.context) as f64,
    };
    (base + cache) as u64
}

/// Where the model runs, for the speed estimate.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpeedHw {
    /// GB/s of the selected GPU(s); `None` = no GPU or a GPU whose bandwidth we don't know.
    pub gpu_bandwidth: Option<f64>,
    /// GB/s of main memory; `None` when the system doesn't report the memory type and rate.
    pub cpu_bandwidth: Option<f64>,
    /// Apple Silicon (Metal): different efficiency than discrete GPUs.
    pub apple: bool,
}

/// Share of the weights read per token: MoE models only use their active experts.
/// From names like "Qwen3-30B-A3B" (3 of 30) or "gpt-oss-120b" (5.1 of 117, known).
pub fn active_ratio(name: &str) -> f64 {
    let u = name.to_ascii_uppercase();
    let words: Vec<&str> = u.split(['-', '_', ' ']).collect();
    let billions = |w: &str| w.strip_suffix('B').and_then(|n| n.parse::<f64>().ok());
    for (i, w) in words.iter().enumerate() {
        if let Some(active) = w.strip_prefix('A').and_then(billions)
            && let Some(total) = words[..i].iter().rev().find_map(|w| billions(w))
            && total > active
        {
            return (active / total).clamp(0.02, 1.0);
        }
    }
    if u.contains("GPT-OSS-120B") {
        return 5.1 / 117.0;
    }
    if u.contains("GPT-OSS-20B") {
        return 3.6 / 21.0;
    }
    1.0
}

/// Estimated generation speed (tokens/s). Each token reads the (active) weights once, so the time
/// per token is bytes ÷ usable bandwidth for the GPU and CPU shares, plus a small fixed cost.
/// Calibrated on: Apple M2 + 4B Q4 ≈ 13–20 t/s, RTX 4090 + 8B Q4 ≈ 110 t/s.
/// `None` when a bandwidth the model needs is unknown: no guessed numbers.
pub fn estimate_tps(size: u64, active: f64, gpu_share: f64, hw: &SpeedHw) -> Option<f64> {
    let gb = size as f64 / GIB * active;
    let gpu_share = gpu_share.clamp(0.0, 1.0);
    let (eff_gpu, overhead) = if hw.apple { (0.6, 0.006) } else { (0.7, 0.002) };
    let gpu_t = if gpu_share > 0.0 { gb * gpu_share / (hw.gpu_bandwidth.filter(|&b| b > 0.0)? * eff_gpu) } else { 0.0 };
    let cpu_t = if gpu_share < 1.0 { gb * (1.0 - gpu_share) / (hw.cpu_bandwidth.filter(|&b| b > 0.0)? * 0.5) } else { 0.0 };
    let t = gpu_t + cpu_t + if gpu_share < 1.0 { 0.006 } else { overhead };
    Some(1.0 / t)
}

/// Quants we prefer, best first (design notes: beginners don't choose).
pub const PREFERRED: &[&str] = &["Q6_K", "Q5_K_M", "Q4_K_M"];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuantPick {
    pub quant: String,
    pub size: u64,
    pub fit: Fit,
    /// A smaller quant than Q4_K_M, picked only because nothing else fits.
    pub fallback: bool,
}

/// Largest of Q6_K / Q5_K_M / Q4_K_M that fits fully (Q8_0 too for small models under 3 GB);
/// otherwise Q4_K_M partly on CPU; otherwise the largest smaller quant that fits, marked as fallback.
pub fn pick_quant(quants: &[(String, u64)], context: u32, vram: u64, ram: u64, unified: bool) -> Option<QuantPick> {
    let find = |q: &str| quants.iter().find(|(n, _)| n.eq_ignore_ascii_case(q)).map(|(_, s)| *s);
    let fit = |s: u64| fit_by_size(s, context, vram, ram, unified);
    let mk = |q: &str, size: u64, fit: Fit, fallback| QuantPick { quant: q.to_string(), size, fit, fallback };

    if let Some(s) = find("Q8_0").filter(|&s| (s as f64) < 3.0 * GIB && matches!(fit(s), Fit::Full | Fit::Cpu)) {
        return Some(mk("Q8_0", s, fit(s), false));
    }
    for q in PREFERRED {
        if let Some(s) = find(q)
            && matches!(fit(s), Fit::Full | Fit::Cpu)
        {
            return Some(mk(q, s, fit(s), false));
        }
    }
    // Other 4-bit quants (e.g. Google's QAT Q4_0 uploads) are a normal choice, not a fallback.
    let four_bit = |n: &str| {
        let u = n.to_ascii_uppercase();
        (u.starts_with("Q4") || u.starts_with("IQ4") || u.starts_with("MXFP4")) && u != "Q4_K_M"
    };
    let mut fours: Vec<&(String, u64)> = quants.iter().filter(|(n, _)| four_bit(n)).collect();
    fours.sort_by_key(|(_, s)| std::cmp::Reverse(*s));
    if let Some((n, s)) = fours.iter().find(|(_, s)| matches!(fit(*s), Fit::Full | Fit::Cpu)) {
        return Some(mk(n, *s, fit(*s), false));
    }
    if let Some(s) = find("Q4_K_M").filter(|&s| fit(s) == Fit::Partial) {
        return Some(mk("Q4_K_M", s, Fit::Partial, false));
    }
    // Fallback: anything smaller that fits at all, largest first.
    // Only quants smaller than Q4_K_M count as fallback (never F16/BF16 or Q5+).
    let smaller = |n: &str| {
        let u = n.to_ascii_uppercase();
        !(u.contains("F16") || u.contains("F32") || u.starts_with("Q5") || u.starts_with("Q6") || u.starts_with("Q8") || PREFERRED.contains(&u.as_str()) || four_bit(&u))
    };
    let mut rest: Vec<&(String, u64)> = quants.iter().filter(|(n, _)| smaller(n)).collect();
    rest.sort_by_key(|(_, s)| std::cmp::Reverse(*s));
    rest.into_iter().find(|(_, s)| fit(*s) != Fit::None).map(|(n, s)| mk(n, *s, fit(*s), true))
}

/// Memory each GPU uses for a split (`--tensor_split`), for the Advanced page bars.
/// Returns bytes per GPU; the main GPU carries ~0.8 GB of extra buffers.
pub fn split_usage(model: &ModelShape, context: u32, shares: &[f64], main_gpu: usize) -> Vec<u64> {
    let total: f64 = shares.iter().sum::<f64>().max(f64::EPSILON);
    let need = model.size as f64 + context_overhead(model.size, context) as f64;
    shares
        .iter()
        .enumerate()
        .map(|(i, s)| (need * s / total + if i == main_gpu { 0.8 * GIB } else { 0.0 }) as u64)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: u64 = 1 << 30;

    fn shape(gb: f64, layers: Option<u32>) -> ModelShape {
        ModelShape { size: (gb * GIB) as u64, layers, head_count_kv: layers.map(|_| 8), kv_len: Some(128) }
    }

    /// Same numbers as KoboldCpp's Python formula, computed by hand:
    /// 8 GB model, 32 layers, 8 KV heads, 16K context, 24 GB GPU, nothing else loaded.
    #[test]
    fn estimate_matches_python() {
        let n = estimate_layers(&shape(8.0, Some(32)), &Extras::default(), &Settings::default(), 24 * G, 0);
        // ratio = max(24/(8*4*1.6)=0.469, (24-1.25-mem1)/(8+mem2)) with mem1=32*4*128*16384*4*1.45 B
        let mem1 = 32.0 * 4.0 * 128.0 * 16384.0 * 4.0 * 1.45;
        let mem2 = 32.0 * 8.0 * 128.0 * 16384.0 * 4.0 * 1.15;
        let ratio = ((24.0 * GIB - 1.25 * GIB - mem1) / (8.0 * GIB + mem2)).max(24.0 / (8.0 * 4.0 * 1.6));
        assert_eq!(n, ((ratio * 32.0) as u32).min(33));
        assert_eq!(n, 33);
    }

    #[test]
    fn small_gpu_gets_some_layers() {
        let n = estimate_layers(&shape(14.0, Some(40)), &Extras::default(), &Settings::default(), 8 * G, 0);
        assert!(n > 2 && n < 40, "{n}");
        let (fit, _) = fit_by_layers(&shape(14.0, Some(40)), &Extras::default(), &Settings::default(), 8 * G, 0, 32 * G, false);
        assert_eq!(fit, Fit::Partial);
    }

    #[test]
    fn extras_reduce_layers() {
        let base = estimate_layers(&shape(14.0, Some(40)), &Extras::default(), &Settings::default(), 12 * G, 0);
        let with = estimate_layers(&shape(14.0, Some(40)), &Extras { image: 6 * G, ..Default::default() }, &Settings::default(), 12 * G, 0);
        assert!(with < base);
    }

    #[test]
    fn no_metadata_uses_size_formula() {
        let n = estimate_layers(&shape(4.0, None), &Extras::default(), &Settings { context: 4096, ..Default::default() }, 8 * G, 0);
        // csmul is 1.2 for 2049..=4096 context
        assert_eq!(n, ((8.0 / (4.0 * 1.2 * 0.052)) as u32).min(200));
    }

    #[test]
    fn quant_pick_prefers_best_that_fits() {
        let q = |v: &[(&str, f64)]| v.iter().map(|(n, g)| (n.to_string(), (g * GIB) as u64)).collect::<Vec<_>>();
        let qwen27 = q(&[("Q6_K", 22.4), ("Q5_K_M", 19.3), ("Q4_K_M", 16.5), ("Q3_K_M", 13.0)]);
        // 19.3 GB + ~2.8 GB context fits in 24 GB; Q6_K (22.4 GB) does not.
        assert_eq!(pick_quant(&qwen27, 16384, 24 * G, 64 * G, false).unwrap().quant, "Q5_K_M");
        let p = pick_quant(&qwen27, 16384, 12 * G, 32 * G, false).unwrap();
        assert_eq!((p.quant.as_str(), p.fit), ("Q4_K_M", Fit::Partial));
        assert_eq!(pick_quant(&qwen27, 16384, 40 * G, 64 * G, false).unwrap().quant, "Q6_K");
        let tiny = q(&[("Q8_0", 1.9), ("Q6_K", 1.5), ("Q4_K_M", 1.1)]);
        assert_eq!(pick_quant(&tiny, 16384, 8 * G, 16 * G, false).unwrap().quant, "Q8_0");
        let p = pick_quant(&q(&[("Q4_K_M", 118.0), ("Q2_K", 64.0)]), 16384, 24 * G, 128 * G, false).unwrap();
        assert_eq!((p.quant.as_str(), p.fit, p.fallback), ("Q2_K", Fit::Partial, true));
        assert!(pick_quant(&q(&[("Q4_K_M", 118.0)]), 16384, 8 * G, 16 * G, false).is_none());
        // Google-style single QAT Q4_0 upload: a normal pick.
        let p = pick_quant(&q(&[("Q4_0", 2.5)]), 16384, 8 * G, 16 * G, false).unwrap();
        assert_eq!((p.quant.as_str(), p.fallback), ("Q4_0", false));
        // A small BF16 helper is never a fallback for a huge model.
        assert!(pick_quant(&q(&[("Q4_K_M", 118.0), ("BF16", 7.9)]), 16384, 8 * G, 16 * G, false).is_none());
    }

    #[test]
    fn apple_unified_memory_is_all_or_nothing() {
        assert_eq!(fit_by_size(10 * G, 16384, 16 * G, 24 * G, true), Fit::Full);
        assert_eq!(fit_by_size(20 * G, 16384, 16 * G, 24 * G, true), Fit::None);
    }

    #[test]
    fn speed_estimates_are_plausible() {
        let m2 = SpeedHw { gpu_bandwidth: Some(100.0), cpu_bandwidth: Some(100.0), apple: true };
        let t = estimate_tps((2.7 * GIB) as u64, 1.0, 1.0, &m2).unwrap();
        assert!((12.0..=25.0).contains(&t), "M2, 4B Q4: {t}");
        let g4090 = SpeedHw { gpu_bandwidth: Some(1008.0), cpu_bandwidth: Some(60.0), apple: false };
        let t = estimate_tps((4.9 * GIB) as u64, 1.0, 1.0, &g4090).unwrap();
        assert!((90.0..=140.0).contains(&t), "4090, 8B Q4: {t}");
        // half on the CPU is much slower
        assert!(estimate_tps((16.0 * GIB) as u64, 1.0, 0.5, &g4090).unwrap() < 10.0);
        // unknown RAM bandwidth: fully on the GPU still has an estimate, anything on the CPU not
        let no_ram = SpeedHw { cpu_bandwidth: None, ..g4090 };
        assert!(estimate_tps((4.9 * GIB) as u64, 1.0, 1.0, &no_ram).is_some());
        assert_eq!(estimate_tps((16.0 * GIB) as u64, 1.0, 0.5, &no_ram), None);
        // unknown GPU: no estimate for anything on it
        let no_gpu_bw = SpeedHw { gpu_bandwidth: None, ..g4090 };
        assert_eq!(estimate_tps((4.9 * GIB) as u64, 1.0, 1.0, &no_gpu_bw), None);
        // MoE: only active experts are read
        assert!((active_ratio("Qwen3-30B-A3B-Instruct") - 0.1).abs() < 1e-9);
        assert_eq!(active_ratio("Qwen3-8B"), 1.0);
        assert!((active_ratio("unsloth/Qwen3-235B-A22B-GGUF") - 22.0 / 235.0).abs() < 1e-9);
        assert!(active_ratio("gpt-oss-20b") < 0.2);
    }

    #[test]
    fn runtime_memory_counts_kv_cache() {
        // 8 GB model, 32 layers, 8 KV heads × 128, 16K context: KV cache = 2×32×8×128×16384×2 B = 2 GiB
        let shape = ModelShape { size: 8 * G, layers: Some(32), head_count_kv: Some(8), kv_len: Some(128) };
        let need = runtime_memory(&shape, &Extras::default(), &Settings::default());
        let expect = 8.0 * GIB + 2.0 * GIB + 0.3 * GIB + 8.0 * GIB * 0.05;
        assert!((need as f64 - expect).abs() < 1e6, "{need} vs {expect}");
        // a vision file adds its share
        let with = runtime_memory(&shape, &Extras { mmproj: G, ..Default::default() }, &Settings::default());
        assert!(with > need);
    }

    #[test]
    fn split_usage_adds_main_gpu_buffers() {
        let u = split_usage(&shape(16.0, Some(64)), 16384, &[60.0, 40.0], 0);
        assert!(u[0] > u[1]);
    }
}
