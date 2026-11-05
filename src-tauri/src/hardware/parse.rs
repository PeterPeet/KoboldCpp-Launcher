//! Pure parsers for the output of GPU tools. Kept free of I/O so they can be tested on
//! recorded output from machines we don't have (NVIDIA, AMD, Vulkan).

const MIB: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedGpu {
    pub name: String,
    pub vram_total: u64,
    pub vram_free: Option<u64>,
    pub discrete: bool,
}

/// `nvidia-smi --query-gpu=name,memory.total,memory.free --format=csv,noheader`
/// Lines like `NVIDIA GeForce RTX 4090, 24564 MiB, 23802 MiB`.
pub fn nvidia_smi(out: &str) -> Vec<ParsedGpu> {
    out.lines()
        .filter_map(|line| {
            let mut parts = line.split(',').map(str::trim);
            let name = parts.next().filter(|n| !n.is_empty())?.to_string();
            let mib = |s: Option<&str>| s.and_then(|s| s.split_whitespace().next()).and_then(|n| n.parse::<u64>().ok());
            let total = mib(parts.next())?;
            let free = mib(parts.next());
            Some(ParsedGpu { name, vram_total: total * MIB, vram_free: free.map(|f| f * MIB), discrete: true })
        })
        .collect()
}

/// `rocminfo`: agents with `Device Type: GPU`, name from `Marketing Name:`, VRAM from the first
/// `Size:` line of its pool list, e.g. `Size: 16760832(0xffc000) KB`.
pub fn rocminfo(out: &str) -> Vec<ParsedGpu> {
    let mut gpus = Vec::new();
    let mut name: Option<String> = None;
    let mut is_gpu = false;
    let mut in_pool = false;
    for line in out.lines().map(str::trim) {
        if line.starts_with("Agent ") {
            name = None;
            is_gpu = false;
            in_pool = false;
        } else if let Some(n) = line.strip_prefix("Marketing Name:") {
            name = Some(n.trim().to_string());
        } else if let Some(t) = line.strip_prefix("Device Type:") {
            is_gpu = t.contains("GPU");
            if is_gpu && let Some(n) = name.clone() {
                gpus.push(ParsedGpu { name: n, vram_total: 0, vram_free: None, discrete: true });
            }
        } else if line.starts_with("Pool Info:") && is_gpu {
            in_pool = true;
        } else if in_pool && is_gpu && let Some(rest) = line.strip_prefix("Size:") {
            let kb = rest.trim().split('(').next().and_then(|n| n.trim().parse::<u64>().ok());
            if let (Some(kb), Some(last)) = (kb, gpus.last_mut())
                && last.vram_total == 0
            {
                last.vram_total = kb * 1024;
            }
            in_pool = false;
        }
    }
    gpus
}

/// `vulkaninfo --summary`: `deviceName = …` and `deviceType = PHYSICAL_DEVICE_TYPE_…` per device.
pub fn vulkan_summary(out: &str) -> Vec<ParsedGpu> {
    let value = |line: &str| line.split_once('=').map(|(_, v)| v.trim().to_string());
    let names: Vec<String> = out.lines().filter(|l| l.contains("deviceName")).filter_map(value).collect();
    let types: Vec<String> = out.lines().filter(|l| l.contains("deviceType")).filter_map(value).collect();
    names
        .into_iter()
        .enumerate()
        .map(|(i, name)| ParsedGpu {
            name,
            vram_total: 0,
            vram_free: None,
            discrete: types.get(i).is_some_and(|t| t == "PHYSICAL_DEVICE_TYPE_DISCRETE_GPU"),
        })
        .collect()
}

/// Full `vulkaninfo`: per device, the largest `MEMORY_HEAP_DEVICE_LOCAL_BIT` heap between
/// 600 MB and 300 GB (same limits as KoboldCpp).
pub fn vulkan_heaps(out: &str) -> Vec<Option<u64>> {
    let (min, max) = (600 * MIB, 300 * 1024 * MIB);
    out.split("VkPhysicalDeviceMemoryProperties")
        .skip(1)
        .map(|chunk| {
            let heaps = chunk.split("memoryTypes:").next().unwrap_or("");
            heaps
                .split("memoryHeaps[")
                .skip(1)
                .filter(|h| h.contains("MEMORY_HEAP_DEVICE_LOCAL_BIT"))
                .filter_map(|h| {
                    let after = h.split("size").nth(1)?;
                    let num: String = after.trim_start_matches([' ', '=']).chars().take_while(char::is_ascii_digit).collect();
                    num.parse::<u64>().ok()
                })
                .filter(|&s| s > min && s < max)
                .max()
        })
        .collect()
}

/// `system_profiler SPDisplaysDataType -json`: GPU model names (e.g. "Apple M2 Pro").
pub fn macos_displays(json: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else { return vec![] };
    v["SPDisplaysDataType"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|d| d["sppci_model"].as_str().or(d["_name"].as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * MIB;

    #[test]
    fn nvidia_two_gpus() {
        let out = "NVIDIA GeForce RTX 4090, 24564 MiB, 23802 MiB\nNVIDIA GeForce RTX 3060, 12288 MiB, 11906 MiB\n";
        let g = nvidia_smi(out);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].name, "NVIDIA GeForce RTX 4090");
        assert_eq!(g[0].vram_total, 24564 * MIB);
        assert_eq!(g[1].vram_free, Some(11906 * MIB));
    }

    #[test]
    fn nvidia_garbage_is_ignored() {
        assert!(nvidia_smi("NVIDIA-SMI has failed because it couldn't communicate with the NVIDIA driver.\n").is_empty());
    }

    #[test]
    fn rocminfo_gpu_agent() {
        let out = include_str!("../../tests/fixtures/rocminfo.txt");
        let g = rocminfo(out);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].name, "AMD Radeon RX 7900 XTX");
        assert_eq!(g[0].vram_total, 25149440 * 1024);
    }

    #[test]
    fn vulkan_summary_and_heaps() {
        let s = include_str!("../../tests/fixtures/vulkaninfo-summary.txt");
        let g = vulkan_summary(s);
        assert_eq!(g.iter().map(|g| (g.name.as_str(), g.discrete)).collect::<Vec<_>>(),
            [("NVIDIA GeForce RTX 3060", true), ("Intel(R) UHD Graphics 770", false)]);
        let full = include_str!("../../tests/fixtures/vulkaninfo-heaps.txt");
        let h = vulkan_heaps(full);
        assert_eq!(h, [Some(12 * GIB), None]);
    }

    #[test]
    fn macos_profiler() {
        let json = r#"{"SPDisplaysDataType":[{"_name":"Apple M2","sppci_cores":"10","sppci_model":"Apple M2","spdisplays_vendor":"sppci_vendor_Apple"}]}"#;
        assert_eq!(macos_displays(json), ["Apple M2"]);
        assert!(macos_displays("not json").is_empty());
    }
}
