//! Hardware detection: GPUs (NVIDIA, AMD, Vulkan, Apple), CPU, RAM.
//!
//! Same tools as KoboldCpp's launcher (`detect_memory_cu`, `detect_memory_vk`), but we keep every
//! GPU with its own memory instead of only the smallest, since the UI shows each one.

pub mod parse;

use crate::config::Backend;
use serde::Serialize;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const GIB: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuApi {
    Cuda,
    Rocm,
    Vulkan,
    Metal,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    /// Device index as KoboldCpp numbers it for this API (`--usecuda normal <i>`, `--usevulkan <i>`).
    pub index: u32,
    pub name: String,
    pub api: GpuApi,
    /// Bytes usable for models. On Apple Silicon this is the Metal budget out of shared RAM.
    pub vram: u64,
    pub vram_free: Option<u64>,
    pub discrete: bool,
    /// Memory bandwidth in GB/s, when the model is known (see `bandwidth_of`).
    pub bandwidth: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Low,
    Mid,
    High,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    pub os: String,
    pub arch: String,
    pub cpu_name: String,
    pub logical_cpus: usize,
    /// KoboldCpp's default `--threads` for this CPU.
    pub default_threads: usize,
    pub avx2: Option<bool>,
    pub ram_total: u64,
    pub ram_available: u64,
    /// Apple Silicon: GPU and CPU share RAM.
    pub unified_memory: bool,
    pub gpus: Vec<Gpu>,
    /// Main memory bandwidth in GB/s (on Apple Silicon the same as the GPU's); `None` when the
    /// system doesn't report the memory type and rate.
    pub ram_bandwidth: Option<f64>,
    /// Memory type and rate as the system reports it, e.g. "DDR4-3200" or "LPDDR5".
    pub ram_type: Option<String>,
    pub suggested_backend: Backend,
    pub tier: Tier,
}

impl HardwareInfo {
    /// Memory of the selected GPUs (indices into `gpus`).
    pub fn vram_of(&self, selected: &[usize]) -> u64 {
        selected.iter().filter_map(|&i| self.gpus.get(i)).map(|g| g.vram).sum()
    }
}

/// Run a tool with a timeout; `None` if missing, failing or too slow.
fn run(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        // No console window flashing up for each tool.
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    // Read in a thread so a large output (full vulkaninfo) can't fill the pipe and block.
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = reader.join().ok()?;
                return status.success().then_some(out);
            }
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(40)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// KoboldCpp's `get_default_threads`: half the logical CPUs, minus one above 3, max 8 on Intel, max 64.
pub fn default_threads(logical: usize, cpu_name: &str) -> usize {
    let physical = if logical > 1 { logical / 2 } else { 1 };
    let mut t = if physical <= 3 { physical } else { (physical - 1).max(3) };
    if cpu_name.contains("Intel") {
        t = t.min(8);
    }
    t.min(64)
}

/// Template tier from KoboldCpp's guide: Low 6 GB, Mid 12 GB, High 24 GB of VRAM.
pub fn tier_for(vram: u64) -> Tier {
    if vram >= 22 * GIB {
        Tier::High
    } else if vram >= 11 * GIB {
        Tier::Mid
    } else {
        Tier::Low
    }
}

/// Metal can use roughly two thirds of RAM on smaller Macs and three quarters on larger ones
/// (macOS `recommendedMaxWorkingSetSize`).
pub fn metal_budget(ram: u64) -> u64 {
    if ram <= 36 * GIB { ram * 2 / 3 } else { ram * 3 / 4 }
}

/// KoboldCpp's backend choice (`auto_set_backend_cli`), applied to what we detected.
pub fn suggest_backend(gpus: &[Gpu], avx2: Option<bool>) -> Backend {
    let cuda_ok = avx2 != Some(false);
    if cuda_ok && gpus.iter().any(|g| matches!(g.api, GpuApi::Cuda | GpuApi::Rocm) && g.vram > 3_500_000_000) {
        return Backend::Cuda { gpu: None, lowvram: false, mmq: true };
    }
    if gpus.iter().any(|g| g.api == GpuApi::Vulkan && g.discrete) {
        return Backend::Vulkan { gpus: vec![] };
    }
    if gpus.iter().any(|g| g.api == GpuApi::Metal) {
        return Backend::Auto;
    }
    Backend::Cpu
}

fn detect_gpus(ram_total: u64) -> Vec<Gpu> {
    let t = Duration::from_secs(10);
    if cfg!(target_os = "macos") {
        let names = run("system_profiler", &["SPDisplaysDataType", "-json"], t).map(|j| parse::macos_displays(&j)).unwrap_or_default();
        let apple_silicon = cfg!(target_arch = "aarch64");
        return names
            .into_iter()
            .enumerate()
            .filter(|_| apple_silicon)
            .map(|(i, name)| Gpu { index: i as u32, bandwidth: bandwidth_of(&name), name, api: GpuApi::Metal, vram: metal_budget(ram_total), vram_free: None, discrete: false })
            .collect();
    }
    let to_gpus = |list: Vec<parse::ParsedGpu>, api: GpuApi| -> Vec<Gpu> {
        list.into_iter()
            .enumerate()
            .map(|(i, g)| Gpu { index: i as u32, bandwidth: bandwidth_of(&g.name), name: g.name, api, vram: g.vram_total, vram_free: g.vram_free, discrete: g.discrete })
            .collect()
    };
    if let Some(out) = run("nvidia-smi", &["--query-gpu=name,memory.total,memory.free", "--format=csv,noheader"], t) {
        let g = parse::nvidia_smi(&out);
        if !g.is_empty() {
            return to_gpus(g, GpuApi::Cuda);
        }
    }
    if let Some(out) = run("rocminfo", &[], t) {
        let g = parse::rocminfo(&out);
        if !g.is_empty() {
            return to_gpus(g, GpuApi::Rocm);
        }
    }
    if let Some(out) = run("vulkaninfo", &["--summary"], t) {
        let mut g = parse::vulkan_summary(&out);
        if g.iter().any(|g| g.discrete)
            && let Some(full) = run("vulkaninfo", &[], t)
        {
            for (gpu, heap) in g.iter_mut().zip(parse::vulkan_heaps(&full)) {
                gpu.vram_total = heap.unwrap_or(0);
            }
        }
        return to_gpus(g, GpuApi::Vulkan);
    }
    vec![]
}

/// Memory type, rate and module count from the system's hardware report.
fn detect_ram() -> Option<parse::RamModules> {
    let t = Duration::from_secs(10);
    if cfg!(target_os = "macos") {
        run("system_profiler", &["SPMemoryDataType", "-json"], t).and_then(|j| parse::macos_memory(&j))
    } else if cfg!(target_os = "windows") {
        let cmd = "Get-CimInstance Win32_PhysicalMemory | Select-Object SMBIOSMemoryType,ConfiguredClockSpeed,Speed | ConvertTo-Json -Compress";
        run("powershell", &["-NoProfile", "-NonInteractive", "-Command", cmd], t).and_then(|j| parse::windows_memory(&j))
    } else {
        run("dmidecode", &["-t", "17"], t).and_then(|o| parse::dmidecode_memory(&o))
    }
}

fn has_avx2() -> Option<bool> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        Some(std::arch::is_x86_feature_detected!("avx2"))
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        None
    }
}

/// Detect everything. Takes up to a few seconds (runs external tools); call off the UI thread.
pub fn detect() -> HardwareInfo {
    let sys = sysinfo::System::new().ok().map(|mut s| {
        s.refresh_memory();
        s.refresh_cpu_all();
        s
    });
    let cpu_name = sys.as_ref().and_then(|s| s.cpus().first().map(|c| c.brand().trim().to_string())).unwrap_or_default();
    let logical = sys.as_ref().map_or(0, |s| s.cpus().len()).max(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1));
    let ram_total = sys.as_ref().map_or(0, |s| s.total_memory());
    let gpus = detect_gpus(ram_total);
    let avx2 = has_avx2();
    let unified = gpus.iter().any(|g| g.api == GpuApi::Metal);
    let best_vram = gpus.iter().map(|g| g.vram).max().unwrap_or(0);
    let ram = detect_ram();
    // Apple Silicon: the chip table (CPU and GPU share the memory). Otherwise from the modules;
    // unknown when the system won't say (no guessed value).
    let ram_bandwidth = gpus
        .iter()
        .find(|g| g.api == GpuApi::Metal)
        .and_then(|g| g.bandwidth)
        .or_else(|| ram.as_ref().and_then(|r| r.bandwidth(&cpu_name)));
    HardwareInfo {
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        default_threads: default_threads(logical, &cpu_name),
        cpu_name,
        logical_cpus: logical,
        avx2,
        ram_total,
        ram_available: sys.as_ref().map_or(0, |s| s.available_memory()),
        unified_memory: unified,
        ram_bandwidth,
        ram_type: ram.and_then(|r| r.label()),
        suggested_backend: suggest_backend(&gpus, avx2),
        tier: tier_for(best_vram),
        gpus,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(api: GpuApi, gb: u64, discrete: bool) -> Gpu {
        Gpu { index: 0, name: "x".into(), api, vram: gb * GIB, vram_free: None, discrete, bandwidth: None }
    }

    #[test]
    fn threads_match_koboldcpp() {
        assert_eq!(default_threads(1, ""), 1);
        assert_eq!(default_threads(4, ""), 2);
        assert_eq!(default_threads(8, "Apple M2"), 3);
        assert_eq!(default_threads(16, "AMD Ryzen 7"), 7);
        assert_eq!(default_threads(32, "13th Gen Intel(R) Core(TM) i9"), 8);
        assert_eq!(default_threads(256, "AMD EPYC"), 64);
    }

    #[test]
    fn tiers() {
        assert_eq!(tier_for(6 * GIB), Tier::Low);
        assert_eq!(tier_for(12 * GIB), Tier::Mid);
        assert_eq!(tier_for(24 * GIB), Tier::High);
    }

    #[test]
    fn backend_choice() {
        assert!(matches!(suggest_backend(&[gpu(GpuApi::Cuda, 12, true)], Some(true)), Backend::Cuda { .. }));
        assert_eq!(suggest_backend(&[gpu(GpuApi::Cuda, 12, true)], Some(false)), Backend::Cpu);
        assert_eq!(suggest_backend(&[gpu(GpuApi::Vulkan, 8, true)], Some(true)), Backend::Vulkan { gpus: vec![] });
        assert_eq!(suggest_backend(&[gpu(GpuApi::Vulkan, 2, false)], Some(true)), Backend::Cpu);
        assert_eq!(suggest_backend(&[gpu(GpuApi::Metal, 10, false)], None), Backend::Auto);
        assert_eq!(suggest_backend(&[], Some(true)), Backend::Cpu);
    }

    #[test]
    fn metal_budget_share() {
        assert_eq!(metal_budget(24 * GIB), 16 * GIB);
        assert_eq!(metal_budget(64 * GIB), 48 * GIB);
    }
}

/// Memory bandwidth in GB/s by device name (published specs). Token generation reads the whole
/// model once per token, so this is what decides the speed.
const BANDWIDTH: &[(&str, f64)] = &[
    // Apple Silicon (unified memory)
    ("M1 ULTRA", 800.0), ("M1 MAX", 400.0), ("M1 PRO", 200.0), ("M1", 68.0),
    ("M2 ULTRA", 800.0), ("M2 MAX", 400.0), ("M2 PRO", 200.0), ("M2", 100.0),
    ("M3 ULTRA", 819.0), ("M3 MAX", 400.0), ("M3 PRO", 150.0), ("M3", 100.0),
    ("M4 MAX", 546.0), ("M4 PRO", 273.0), ("M4", 120.0), ("M5", 153.0),
    // NVIDIA
    ("RTX 5090", 1792.0), ("RTX 5080", 960.0), ("RTX 5070 TI", 896.0), ("RTX 5070", 672.0), ("RTX 5060 TI", 448.0), ("RTX 5060", 448.0),
    ("RTX 4090", 1008.0), ("RTX 4080 SUPER", 736.0), ("RTX 4080", 717.0), ("RTX 4070 TI SUPER", 672.0), ("RTX 4070 TI", 504.0),
    ("RTX 4070 SUPER", 504.0), ("RTX 4070", 504.0), ("RTX 4060 TI", 288.0), ("RTX 4060", 272.0),
    ("RTX 3090 TI", 1008.0), ("RTX 3090", 936.0), ("RTX 3080 TI", 912.0), ("RTX 3080", 760.0), ("RTX 3070 TI", 608.0),
    ("RTX 3070", 448.0), ("RTX 3060 TI", 448.0), ("RTX 3060", 360.0), ("RTX 3050", 224.0),
    ("RTX 2080 TI", 616.0), ("RTX 2080", 448.0), ("RTX 2070", 448.0), ("RTX 2060", 336.0),
    ("GTX 1080 TI", 484.0), ("GTX 1080", 320.0), ("GTX 1070", 256.0), ("GTX 1660 SUPER", 336.0), ("GTX 1660", 192.0),
    ("RTX 6000 ADA", 960.0), ("RTX A6000", 768.0), ("RTX A4000", 448.0), ("H100", 3350.0), ("A100", 1555.0),
    ("L40S", 864.0), ("V100", 900.0), ("P40", 346.0),
    // AMD
    ("RX 9070 XT", 640.0), ("RX 9070", 640.0), ("RX 7900 XTX", 960.0), ("RX 7900 XT", 800.0), ("RX 7800 XT", 624.0),
    ("RX 7700 XT", 432.0), ("RX 7600", 288.0), ("RX 6950 XT", 576.0), ("RX 6900 XT", 512.0), ("RX 6800 XT", 512.0),
    ("RX 6800", 512.0), ("RX 6700 XT", 384.0), ("RX 6600 XT", 256.0), ("MI300X", 5300.0),
    // Intel
    ("ARC A770", 560.0), ("ARC A750", 512.0), ("ARC B580", 456.0),
];

/// Bandwidth of a GPU by name; the most specific (longest) match wins. `None` if unknown.
pub fn bandwidth_of(name: &str) -> Option<f64> {
    let n = name.to_ascii_uppercase();
    let words: Vec<&str> = n.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
    // Match whole words, so "M2" doesn't match "M2 PRO" and "RTX 4070" doesn't match "4070 TI".
    let has = |key: &str| {
        let k: Vec<&str> = key.split(' ').collect();
        words.windows(k.len()).any(|w| w == k.as_slice()) && {
            // the next word after the match must not make it a bigger model ("PRO", "MAX", "TI", …)
            let pos = words.windows(k.len()).position(|w| w == k.as_slice()).unwrap();
            !matches!(words.get(pos + k.len()), Some(&("PRO" | "MAX" | "ULTRA" | "TI" | "SUPER" | "XT" | "XTX")))
        }
    };
    BANDWIDTH.iter().filter(|(k, _)| has(k)).max_by_key(|(k, _)| k.len()).map(|(_, b)| *b)
}

#[cfg(test)]
mod bandwidth_tests {
    use super::bandwidth_of;

    #[test]
    fn names() {
        assert_eq!(bandwidth_of("Apple M2"), Some(100.0));
        assert_eq!(bandwidth_of("Apple M2 Pro"), Some(200.0));
        assert_eq!(bandwidth_of("Apple M4 Max"), Some(546.0));
        assert_eq!(bandwidth_of("NVIDIA GeForce RTX 4090"), Some(1008.0));
        assert_eq!(bandwidth_of("NVIDIA GeForce RTX 4070 Ti"), Some(504.0));
        assert_eq!(bandwidth_of("NVIDIA GeForce RTX 4070"), Some(504.0));
        assert_eq!(bandwidth_of("NVIDIA GeForce RTX 3060 Ti"), Some(448.0));
        assert_eq!(bandwidth_of("AMD Radeon RX 7900 XTX"), Some(960.0));
        assert_eq!(bandwidth_of("Intel(R) UHD Graphics 770"), None);
    }
}
