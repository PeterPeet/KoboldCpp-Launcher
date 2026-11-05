//! Reads the metadata header of a GGUF file (format: github.com/ggml-org/ggml/blob/master/docs/gguf.md).
//!
//! Only the key/value section is read; tensor data is never touched, so this is fast even for
//! 100 GB files. Sizes are bounded so a corrupt file can't make us allocate gigabytes.

use serde::Serialize;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};

const MAX_STRING: u64 = 64 * 1024 * 1024;
const MAX_KV: u64 = 1_000_000;
const MAX_ARRAY: u64 = 50_000_000;

#[derive(Debug, thiserror::Error)]
pub enum GgufError {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("not a GGUF file")]
    NotGguf,
    #[error("GGUF version {0} is not supported")]
    Version(u32),
    #[error("GGUF header is corrupt ({0})")]
    Corrupt(&'static str),
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GgufInfo {
    pub version: u32,
    pub architecture: Option<String>,
    pub name: Option<String>,
    pub size_label: Option<String>,
    /// Quantization name from `general.file_type`, e.g. "Q4_K_M".
    pub file_type: Option<String>,
    pub block_count: Option<u32>,
    pub context_length: Option<u64>,
    pub embedding_length: Option<u64>,
    /// Largest per-layer value when the file stores a list.
    pub head_count_kv: Option<u32>,
    pub key_length: Option<u32>,
    pub value_length: Option<u32>,
    pub expert_count: Option<u32>,
    /// Multi-token-prediction layers in the file (`<arch>.nextn_predict_layers`); KoboldCpp can
    /// use them for faster answers with `--usemtp`.
    pub mtp_layers: Option<u32>,
    pub tensor_count: u64,
    /// Sum of all parts for split files (`-00001-of-00003.gguf`).
    pub total_size: u64,
    pub parts: u32,
}

#[derive(Debug, Clone)]
enum Val {
    Int(i128),
    Float(f64),
    Str(String),
    /// Numbers from an array (other arrays are skipped).
    Ints(Vec<i128>),
    Other,
}

struct R<T: Read>(T);

impl<T: Read> R<T> {
    fn buf<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        let mut b = [0u8; N];
        self.0.read_exact(&mut b)?;
        Ok(b)
    }
    fn u8(&mut self) -> io::Result<u8> { Ok(self.buf::<1>()?[0]) }
    fn u16(&mut self) -> io::Result<u16> { Ok(u16::from_le_bytes(self.buf()?)) }
    fn u32(&mut self) -> io::Result<u32> { Ok(u32::from_le_bytes(self.buf()?)) }
    fn u64(&mut self) -> io::Result<u64> { Ok(u64::from_le_bytes(self.buf()?)) }

    fn skip(&mut self, n: u64) -> io::Result<()> {
        let copied = io::copy(&mut (&mut self.0).take(n), &mut io::sink())?;
        if copied < n { Err(io::ErrorKind::UnexpectedEof.into()) } else { Ok(()) }
    }

    fn string(&mut self) -> Result<String, GgufError> {
        let len = self.u64()?;
        if len > MAX_STRING {
            return Err(GgufError::Corrupt("string too long"));
        }
        let mut b = vec![0u8; len as usize];
        self.0.read_exact(&mut b)?;
        Ok(String::from_utf8_lossy(&b).into_owned())
    }

    fn value(&mut self, ty: u32, keep: bool) -> Result<Val, GgufError> {
        Ok(match ty {
            0 => Val::Int(self.u8()? as i128),
            1 => Val::Int(self.u8()? as i8 as i128),
            2 => Val::Int(self.u16()? as i128),
            3 => Val::Int(self.u16()? as i16 as i128),
            4 => Val::Int(self.u32()? as i128),
            5 => Val::Int(self.u32()? as i32 as i128),
            6 => Val::Float(f32::from_le_bytes(self.buf()?) as f64),
            7 => {
                self.u8()?;
                Val::Other
            }
            8 => {
                if keep { Val::Str(self.string()?) } else {
                    let len = self.u64()?;
                    if len > MAX_STRING { return Err(GgufError::Corrupt("string too long")); }
                    self.skip(len)?;
                    Val::Other
                }
            }
            9 => {
                let elem = self.u32()?;
                let count = self.u64()?;
                if count > MAX_ARRAY {
                    return Err(GgufError::Corrupt("array too long"));
                }
                let fixed = match elem { 0 | 1 | 7 => Some(1), 2 | 3 => Some(2), 4..=6 => Some(4), 10..=12 => Some(8), _ => None };
                if keep && matches!(elem, 0..=5 | 10 | 11) && count <= 4096 {
                    let mut v = Vec::with_capacity(count as usize);
                    for _ in 0..count {
                        if let Val::Int(i) = self.value(elem, false)? { v.push(i) }
                    }
                    Val::Ints(v)
                } else if let Some(size) = fixed {
                    self.skip(count * size)?;
                    Val::Other
                } else {
                    for _ in 0..count {
                        self.value(elem, false)?;
                    }
                    Val::Other
                }
            }
            10 => Val::Int(self.u64()? as i128),
            11 => Val::Int(self.u64()? as i64 as i128),
            12 => Val::Float(f64::from_le_bytes(self.buf()?)),
            _ => return Err(GgufError::Corrupt("unknown value type")),
        })
    }
}

impl Val {
    fn as_u64(&self) -> Option<u64> {
        match self {
            Val::Int(i) if *i >= 0 => u64::try_from(*i).ok(),
            Val::Ints(v) => v.iter().copied().max().and_then(|m| u64::try_from(m).ok()),
            Val::Float(f) if *f >= 0.0 => Some(*f as u64),
            _ => None,
        }
    }
    fn as_u32(&self) -> Option<u32> { self.as_u64().and_then(|v| u32::try_from(v).ok()) }
    fn into_string(self) -> Option<String> { if let Val::Str(s) = self { Some(s) } else { None } }
}

/// `llama_ftype` numbers to the usual quant names.
pub fn file_type_name(ft: u32) -> Option<&'static str> {
    Some(match ft {
        0 => "F32", 1 => "F16", 2 => "Q4_0", 3 => "Q4_1", 7 => "Q8_0", 8 => "Q5_0", 9 => "Q5_1",
        10 => "Q2_K", 11 => "Q3_K_S", 12 => "Q3_K_M", 13 => "Q3_K_L", 14 => "Q4_K_S", 15 => "Q4_K_M",
        16 => "Q5_K_S", 17 => "Q5_K_M", 18 => "Q6_K", 19 => "IQ2_XXS", 20 => "IQ2_XS", 21 => "Q2_K_S",
        22 => "IQ3_XS", 23 => "IQ3_XXS", 24 => "IQ1_S", 25 => "IQ4_NL", 26 => "IQ3_S", 27 => "IQ3_M",
        28 => "IQ2_S", 29 => "IQ2_M", 30 => "IQ4_XS", 31 => "IQ1_M", 32 => "BF16", 36 => "TQ1_0",
        37 => "TQ2_0", 38 => "MXFP4_MOE",
        _ => return None,
    })
}

/// `-00001-of-00003` in a file name → (part, total).
pub fn split_part(file_name: &str) -> Option<(u32, u32)> {
    let i = file_name.find("-of-")?;
    let part = file_name.get(i.checked_sub(5)?..i)?;
    let total = file_name.get(i + 4..i + 9)?;
    if part.chars().all(|c| c.is_ascii_digit()) && total.chars().all(|c| c.is_ascii_digit()) && file_name.as_bytes().get(i - 6) == Some(&b'-') {
        Some((part.parse().ok()?, total.parse().ok()?))
    } else {
        None
    }
}

/// Paths of all parts of a split model, given any part. Single files return just themselves.
pub fn split_paths(path: &Path) -> Vec<PathBuf> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let Some((part, total)) = split_part(name) else { return vec![path.to_path_buf()] };
    let tag = format!("-{part:05}-of-{total:05}");
    (1..=total).map(|i| path.with_file_name(name.replace(&tag, &format!("-{i:05}-of-{total:05}")))).collect()
}

pub fn read_header<T: Read>(input: T) -> Result<GgufInfo, GgufError> {
    header(input, false).map(|(info, _)| info)
}

/// Metadata from the start of a file only (e.g. the first megabyte via an HTTP range request).
/// Writers put the model's own keys before the tokenizer's long word lists, so reading stops at
/// the tokenizer. `complete` is false when the data ended before that point: keys not found are
/// then unknown, not absent.
pub fn read_header_prefix(bytes: &[u8]) -> Result<(GgufInfo, bool), GgufError> {
    header(bytes, true)
}

fn header<T: Read>(input: T, prefix: bool) -> Result<(GgufInfo, bool), GgufError> {
    let mut r = R(input);
    if &r.buf::<4>()? != b"GGUF" {
        return Err(GgufError::NotGguf);
    }
    let version = r.u32()?;
    if !(2..=3).contains(&version) {
        return Err(GgufError::Version(version));
    }
    let tensor_count = r.u64()?;
    let kv_count = r.u64()?;
    if kv_count > MAX_KV {
        return Err(GgufError::Corrupt("too many keys"));
    }
    let mut info = GgufInfo { version, tensor_count, ..Default::default() };
    let mut arch_keys: Vec<(String, Val)> = Vec::new();
    let mut complete = true;
    let eof = |e: &GgufError| matches!(e, GgufError::Io(x) if x.kind() == io::ErrorKind::UnexpectedEof);
    for _ in 0..kv_count {
        // Prefix mode: the end of the data is fine once the model's keys are behind us.
        macro_rules! read {
            ($e:expr) => {
                match $e {
                    Ok(v) => v,
                    Err(e) => {
                        let e = GgufError::from(e);
                        if prefix && eof(&e) {
                            complete = false;
                            break;
                        }
                        return Err(e);
                    }
                }
            };
        }
        let key = read!(r.string());
        if prefix && key.starts_with("tokenizer.") && arch_keys.iter().any(|(k, _)| k.ends_with(".block_count")) {
            break;
        }
        let ty = read!(r.u32());
        let wanted = key.starts_with("general.") || key.ends_with(".block_count") || key.ends_with(".context_length")
            || key.ends_with(".embedding_length") || key.ends_with(".attention.head_count_kv")
            || key.ends_with(".attention.key_length") || key.ends_with(".attention.value_length") || key.ends_with(".expert_count") || key.ends_with(".nextn_predict_layers");
        let val = read!(r.value(ty, wanted));
        if !wanted {
            continue;
        }
        match key.as_str() {
            "general.architecture" => info.architecture = val.into_string(),
            "general.name" => info.name = val.into_string(),
            "general.size_label" => info.size_label = val.into_string(),
            "general.file_type" => info.file_type = val.as_u32().and_then(file_type_name).map(str::to_string),
            _ => arch_keys.push((key, val)),
        }
    }
    if let Some(arch) = info.architecture.clone() {
        let get = |suffix: &str| arch_keys.iter().find(|(k, _)| *k == format!("{arch}.{suffix}")).map(|(_, v)| v);
        info.block_count = get("block_count").and_then(Val::as_u32);
        info.context_length = get("context_length").and_then(Val::as_u64);
        info.embedding_length = get("embedding_length").and_then(Val::as_u64);
        info.head_count_kv = get("attention.head_count_kv").and_then(Val::as_u32);
        info.key_length = get("attention.key_length").and_then(Val::as_u32);
        info.value_length = get("attention.value_length").and_then(Val::as_u32);
        info.expert_count = get("expert_count").and_then(Val::as_u32);
        info.mtp_layers = get("nextn_predict_layers").and_then(Val::as_u32);
    }
    Ok((info, complete))
}

/// Read metadata from a file and total up the sizes of all split parts that exist.
pub fn read_file(path: &Path) -> Result<GgufInfo, GgufError> {
    let mut info = read_header(BufReader::with_capacity(1 << 16, File::open(path)?))?;
    let parts = split_paths(path);
    info.parts = parts.len() as u32;
    info.total_size = parts.iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum();
    Ok(info)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Build a small GGUF header for tests.
    #[derive(Default)]
    pub struct Builder {
        kv: Vec<u8>,
        count: u64,
    }

    impl Builder {
        pub fn new() -> Self { Self::default() }
        fn key(&mut self, k: &str, ty: u32) {
            self.kv.extend((k.len() as u64).to_le_bytes());
            self.kv.extend(k.as_bytes());
            self.kv.extend(ty.to_le_bytes());
            self.count += 1;
        }
        pub fn str(mut self, k: &str, v: &str) -> Self {
            self.key(k, 8);
            self.kv.extend((v.len() as u64).to_le_bytes());
            self.kv.extend(v.as_bytes());
            self
        }
        pub fn u32(mut self, k: &str, v: u32) -> Self {
            self.key(k, 4);
            self.kv.extend(v.to_le_bytes());
            self
        }
        pub fn u64(mut self, k: &str, v: u64) -> Self {
            self.key(k, 10);
            self.kv.extend(v.to_le_bytes());
            self
        }
        pub fn i32_array(mut self, k: &str, v: &[i32]) -> Self {
            self.key(k, 9);
            self.kv.extend(5u32.to_le_bytes());
            self.kv.extend((v.len() as u64).to_le_bytes());
            for x in v { self.kv.extend(x.to_le_bytes()); }
            self
        }
        pub fn str_array(mut self, k: &str, v: &[&str]) -> Self {
            self.key(k, 9);
            self.kv.extend(8u32.to_le_bytes());
            self.kv.extend((v.len() as u64).to_le_bytes());
            for s in v {
                self.kv.extend((s.len() as u64).to_le_bytes());
                self.kv.extend(s.as_bytes());
            }
            self
        }
        pub fn build(self) -> Vec<u8> {
            let mut out = b"GGUF".to_vec();
            out.extend(3u32.to_le_bytes());
            out.extend(291u64.to_le_bytes());
            out.extend(self.count.to_le_bytes());
            out.extend(self.kv);
            out
        }
    }

    pub fn llama_like() -> Vec<u8> {
        Builder::new()
            .str("general.architecture", "llama")
            .str("general.name", "Tiny Llama")
            .u32("general.file_type", 15)
            .str_array("tokenizer.ggml.tokens", &["<s>", "</s>", "hello"])
            .u32("llama.block_count", 32)
            .u64("llama.context_length", 131072)
            .u32("llama.embedding_length", 4096)
            .u32("llama.attention.head_count_kv", 8)
            .u32("llama.attention.key_length", 128)
            .u32("llama.nextn_predict_layers", 1)
            .build()
    }

    #[test]
    fn reads_metadata_and_skips_arrays() {
        let info = read_header(&llama_like()[..]).unwrap();
        assert_eq!(info.architecture.as_deref(), Some("llama"));
        assert_eq!(info.file_type.as_deref(), Some("Q4_K_M"));
        assert_eq!(info.block_count, Some(32));
        assert_eq!(info.context_length, Some(131072));
        assert_eq!(info.head_count_kv, Some(8));
        assert_eq!(info.key_length, Some(128));
        assert_eq!(info.mtp_layers, Some(1));
        assert_eq!(info.tensor_count, 291);
    }

    #[test]
    fn prefix_stops_at_tokenizer_and_knows_when_it_is_cut() {
        // Usual order: model keys, then the tokenizer's long lists.
        let file = Builder::new()
            .str("general.architecture", "gemma4")
            .u32("gemma4.block_count", 35)
            .str_array("tokenizer.ggml.tokens", &["a"; 500])
            .build();
        let (info, complete) = read_header_prefix(&file[..160]).unwrap();
        assert!(complete, "cut inside the tokenizer: model keys are all read");
        assert_eq!((info.block_count, info.mtp_layers), (Some(35), None));
        let with_mtp = Builder::new().str("general.architecture", "qwen35").u32("qwen35.block_count", 65).u32("qwen35.nextn_predict_layers", 1).str_array("tokenizer.ggml.tokens", &["a"; 500]).build();
        assert_eq!(read_header_prefix(&with_mtp[..200]).unwrap().0.mtp_layers, Some(1));
        // Cut before the model keys end: unknown.
        assert!(!read_header_prefix(&file[..50]).unwrap().1);
        // Tokenizer first (llama_like): a cut inside it says nothing about the keys after it.
        let odd = llama_like();
        assert!(!read_header_prefix(&odd[..195]).unwrap().1);
        assert_eq!(read_header_prefix(&odd).unwrap().0.mtp_layers, Some(1));
    }

    #[test]
    fn per_layer_kv_heads_use_max() {
        let bytes = Builder::new().str("general.architecture", "gemma3").i32_array("gemma3.attention.head_count_kv", &[4, 8, 4]).build();
        assert_eq!(read_header(&bytes[..]).unwrap().head_count_kv, Some(8));
    }

    #[test]
    fn rejects_other_files_and_truncation() {
        assert!(matches!(read_header(&b"PK\x03\x04xxxxxxxxxxxxxxxxxxxx"[..]), Err(GgufError::NotGguf)));
        let full = llama_like();
        assert!(read_header(&full[..full.len() - 3]).is_err());
        let mut huge = b"GGUF".to_vec();
        huge.extend(3u32.to_le_bytes());
        huge.extend(0u64.to_le_bytes());
        huge.extend(u64::MAX.to_le_bytes());
        assert!(matches!(read_header(&huge[..]), Err(GgufError::Corrupt(_))));
    }

    #[test]
    fn split_names() {
        assert_eq!(split_part("Qwen3-235B-Q4_K_M-00002-of-00004.gguf"), Some((2, 4)));
        assert_eq!(split_part("model.gguf"), None);
        let p = split_paths(Path::new("/m/x-00001-of-00003.gguf"));
        assert_eq!(p.len(), 3);
        assert_eq!(p[2], PathBuf::from("/m/x-00003-of-00003.gguf"));
    }

    #[test]
    fn reads_file_and_sums_parts() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("m-00001-of-00002.gguf");
        std::fs::write(&p1, llama_like()).unwrap();
        std::fs::write(dir.path().join("m-00002-of-00002.gguf"), vec![0u8; 1000]).unwrap();
        let info = read_file(&p1).unwrap();
        assert_eq!(info.parts, 2);
        assert_eq!(info.total_size, llama_like().len() as u64 + 1000);
    }
}

#[cfg(test)]
mod real_file {
    /// Point `KL_GGUF` at a real model (or its first few MB) and run `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn real_file_header() {
        let p = std::env::var("KL_GGUF").expect("set KL_GGUF");
        let info = super::read_header(std::io::BufReader::new(std::fs::File::open(p).unwrap())).unwrap();
        println!("{info:#?}");
        assert!(info.architecture.is_some() && info.block_count.is_some());
    }
}
