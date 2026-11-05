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

/// Installed memory as the system reports it: type, transfer rate, number of modules.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RamModules {
    /// "DDR4", "DDR5", "LPDDR5", ...
    pub kind: Option<String>,
    /// Transfer rate in MT/s (what is sold as "DDR4 3200").
    pub mts: Option<u32>,
    /// Populated modules (memory sticks or soldered packages).
    pub modules: u32,
}

impl RamModules {
    /// "DDR4-3200", or just "LPDDR5" when the speed is unknown.
    pub fn label(&self) -> Option<String> {
        let kind = self.kind.as_deref()?;
        Some(match self.mts {
            Some(m) => format!("{kind}-{m}"),
            None => kind.to_string(),
        })
    }

    /// Peak bandwidth in GB/s: rate × bus width. A memory channel is 64 bits (8 bytes); desktop
    /// CPUs have two channels, workstation and server CPUs more. Soldered LPDDR in laptops is
    /// almost always a 128-bit bus, however many packages the system lists.
    pub fn bandwidth(&self, cpu_name: &str) -> Option<f64> {
        let mts = f64::from(self.mts?);
        let kind = self.kind.as_deref().unwrap_or("");
        let bytes = if kind.starts_with("LPDDR") {
            16.0
        } else {
            let server = ["Threadripper", "Xeon", "EPYC"].iter().any(|n| cpu_name.contains(n));
            let channels = self.modules.clamp(1, if server { 8 } else { 2 });
            8.0 * f64::from(channels)
        };
        Some(mts * bytes / 1000.0)
    }
}

/// Some systems report the clock (MHz) instead of the transfer rate (MT/s): half the number.
fn fix_rate(kind: &str, value: u32) -> u32 {
    let lowest = match kind {
        "DDR3" | "LPDDR3" => 800,
        "DDR4" | "LPDDR4" | "LPDDR4X" => 1600,
        "DDR5" | "LPDDR5" | "LPDDR5X" => 3200,
        _ => 0,
    };
    if value > 0 && value < lowest { value * 2 } else { value }
}

/// SMBIOS memory type codes (Windows `Win32_PhysicalMemory.SMBIOSMemoryType`).
fn smbios_kind(code: u64) -> Option<&'static str> {
    Some(match code {
        18 => "DDR",
        19 => "DDR2",
        24 => "DDR3",
        26 => "DDR4",
        27 => "LPDDR",
        28 => "LPDDR2",
        29 => "LPDDR3",
        30 => "LPDDR4",
        34 => "DDR5",
        35 => "LPDDR5",
        _ => return None,
    })
}

fn summarize(list: Vec<(Option<String>, Option<u32>)>) -> Option<RamModules> {
    if list.is_empty() {
        return None;
    }
    let kind = list.iter().find_map(|(k, _)| k.clone());
    // Mixed modules all run at the slowest one's rate.
    let mts = list.iter().filter_map(|(_, m)| *m).filter(|&m| m > 0).min().map(|m| fix_rate(kind.as_deref().unwrap_or(""), m));
    Some(RamModules { kind, mts, modules: list.len() as u32 })
}

/// Windows: `Get-CimInstance Win32_PhysicalMemory | Select-Object SMBIOSMemoryType,ConfiguredClockSpeed,Speed | ConvertTo-Json`
/// (one object for one module, an array for several).
pub fn windows_memory(json: &str) -> Option<RamModules> {
    let v: serde_json::Value = serde_json::from_str(json.trim()).ok()?;
    let items = match v {
        serde_json::Value::Array(a) => a,
        o @ serde_json::Value::Object(_) => vec![o],
        _ => return None,
    };
    summarize(
        items
            .iter()
            .map(|m| {
                let kind = m["SMBIOSMemoryType"].as_u64().and_then(smbios_kind).map(str::to_string);
                let speed = m["ConfiguredClockSpeed"].as_u64().filter(|&s| s > 0).or(m["Speed"].as_u64());
                (kind, speed.map(|s| s as u32))
            })
            .collect(),
    )
}

/// Linux: `dmidecode -t 17` (needs root; without it there is no output and we keep the default).
pub fn dmidecode_memory(out: &str) -> Option<RamModules> {
    let mut list = vec![];
    for block in out.split("Memory Device").skip(1) {
        let field = |name: &str| {
            block.lines().map(str::trim).find_map(|l| l.strip_prefix(name).and_then(|r| r.strip_prefix(':')).map(str::trim))
        };
        let size = field("Size").unwrap_or("");
        if size.is_empty() || size.starts_with("No Module") || size.starts_with("Not Installed") {
            continue;
        }
        let kind = field("Type").filter(|t| !t.is_empty() && *t != "Unknown" && *t != "Other").map(str::to_string);
        let rate = |name| field(name).and_then(|s| s.split_whitespace().next()?.parse::<u32>().ok());
        list.push((kind, rate("Configured Memory Speed").or(rate("Configured Clock Speed")).or(rate("Speed"))));
    }
    summarize(list)
}

/// macOS: `system_profiler SPMemoryDataType -json`. Apple Silicon lists one entry with the type
/// only; Intel Macs list each slot with type and speed ("2667 MHz", which is the rate in MT/s).
pub fn macos_memory(json: &str) -> Option<RamModules> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let top = v["SPMemoryDataType"].as_array()?;
    let mut list = vec![];
    for entry in top {
        let slots = entry["_items"].as_array().cloned().unwrap_or_else(|| vec![entry.clone()]);
        for s in slots {
            let size = s["dimm_size"].as_str().or(s["SPMemoryDataType"].as_str()).unwrap_or("");
            if size.is_empty() || size.eq_ignore_ascii_case("empty") {
                continue;
            }
            let kind = s["dimm_type"].as_str().filter(|t| !t.eq_ignore_ascii_case("empty")).map(str::to_string);
            let mts = s["dimm_speed"].as_str().and_then(|x| x.split_whitespace().next()?.parse().ok());
            list.push((kind, mts));
        }
    }
    summarize(list)
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

    #[test]
    fn windows_ddr4_dual_channel() {
        let json = r#"[{"SMBIOSMemoryType":26,"ConfiguredClockSpeed":3200,"Speed":3600},{"SMBIOSMemoryType":26,"ConfiguredClockSpeed":3200,"Speed":3600}]"#;
        let r = windows_memory(json).unwrap();
        assert_eq!(r.label().as_deref(), Some("DDR4-3200"));
        assert_eq!(r.bandwidth("AMD Ryzen 7 5800X"), Some(51.2));
        // One stick: single channel.
        let one = windows_memory(r#"{"SMBIOSMemoryType":34,"ConfiguredClockSpeed":2400,"Speed":4800}"#).unwrap();
        assert_eq!(one.label().as_deref(), Some("DDR5-4800"), "clock in MHz is doubled to MT/s");
        assert_eq!(one.bandwidth("Intel Core i5"), Some(38.4));
        // Four sticks on a desktop CPU are still two channels.
        let four = RamModules { kind: Some("DDR5".into()), mts: Some(6000), modules: 4 };
        assert_eq!(four.bandwidth("AMD Ryzen 9 7950X"), Some(96.0));
        assert_eq!(four.bandwidth("AMD Ryzen Threadripper 7970X"), Some(192.0));
        let lp = RamModules { kind: Some("LPDDR5".into()), mts: Some(6400), modules: 8 };
        assert_eq!(lp.bandwidth("AMD Ryzen 7 7840U"), Some(102.4));
    }

    #[test]
    fn linux_dmidecode() {
        let out = "Handle 0x0040, DMI type 17, 92 bytes\nMemory Device\n\tSize: 16 GB\n\tType: DDR4\n\tSpeed: 3600 MT/s\n\tConfigured Memory Speed: 3200 MT/s\n\nHandle 0x0041, DMI type 17, 92 bytes\nMemory Device\n\tSize: No Module Installed\n\tType: Unknown\n\tSpeed: Unknown\n\nHandle 0x0042, DMI type 17, 92 bytes\nMemory Device\n\tSize: 16 GB\n\tType: DDR4\n\tSpeed: 3600 MT/s\n\tConfigured Memory Speed: 3200 MT/s\n";
        let r = dmidecode_memory(out).unwrap();
        assert_eq!((r.label().as_deref(), r.modules), (Some("DDR4-3200"), 2));
        assert!(dmidecode_memory("# dmidecode 3.5\n/sys/firmware/dmi/tables/smbios_entry_point: Permission denied\n").is_none());
    }

    #[test]
    fn macos_memory_types() {
        let apple = r#"{"SPMemoryDataType":[{"dimm_manufacturer":"Hynix","dimm_type":"LPDDR5","SPMemoryDataType":"8 GB"}]}"#;
        let r = macos_memory(apple).unwrap();
        assert_eq!((r.label().as_deref(), r.mts), (Some("LPDDR5"), None));
        let intel = r#"{"SPMemoryDataType":[{"_name":"Memory Slots","_items":[{"_name":"DIMM0","dimm_size":"16 GB","dimm_speed":"2667 MHz","dimm_type":"DDR4"},{"_name":"DIMM1","dimm_size":"16 GB","dimm_speed":"2667 MHz","dimm_type":"DDR4"}]}]}"#;
        let r = macos_memory(intel).unwrap();
        assert_eq!(r.label().as_deref(), Some("DDR4-2667"));
        assert_eq!(r.bandwidth("Intel(R) Core(TM) i9"), Some(42.672));
    }
}
