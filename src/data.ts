// Sample data from the mockup. Template names come from hf.co/koboldcpp/newbie-templates;
// models, sizes and numbers are examples until the Rust backend supplies real data.

export type Fit = "full" | "partial" | "cpu" | "none";
export type TierId = "low" | "mid" | "high";

export interface Gpu { name: string; vram: number }
export interface Tier { label: string; gpus: Gpu[]; ram: number; cpu: string }

export const TIERS: Record<TierId, Tier> = {
  low: { label: "Low spec", gpus: [{ name: "GTX 1660 Super", vram: 6 }], ram: 16, cpu: "Ryzen 5 3600" },
  mid: { label: "Mid spec", gpus: [{ name: "RTX 4070", vram: 12 }], ram: 32, cpu: "Ryzen 7 7700" },
  high: { label: "High spec", gpus: [{ name: "RTX 4090", vram: 24 }, { name: "RTX 3060", vram: 12 }], ram: 64, cpu: "Ryzen 9 7950X" },
};

export interface Template { id: string; tier: TierId; use: string; size: number; parts: string[]; ctx?: string }

export const TEMPLATES: Template[] = [
  { id: "LowSpec-Chatbot", tier: "low", use: "Chatbot", size: 4.1, parts: ["Text", "Voice in", "Voice out"] },
  { id: "LowSpec-CodingAgent", tier: "low", use: "Coding agent", size: 4.6, parts: ["Text", "Tools"] },
  { id: "LowSpec-ImageGen", tier: "low", use: "Images", size: 3.8, parts: ["Image model"] },
  { id: "LowSpec-MusicGen", tier: "low", use: "Music", size: 4.4, parts: ["Music model"] },
  { id: "MidSpec-Chatbot", tier: "mid", use: "Chatbot", size: 7.6, parts: ["Qwen3-VL-8B", "Sees images", "Voice in", "Voice out", "Web search"], ctx: "24K context" },
  { id: "MidSpec-CodingAgent", tier: "mid", use: "Coding agent", size: 9.8, parts: ["Text", "Tools", "Long context"] },
  { id: "MidSpec-ImageGen", tier: "mid", use: "Images", size: 8.9, parts: ["Image model", "Image editing"] },
  { id: "HighSpec-CodingAgent", tier: "high", use: "Coding agent", size: 19.5, parts: ["Text", "Tools", "Long context"] },
  { id: "HighSpec-VideoGen", tier: "high", use: "Video", size: 22.0, parts: ["Video model"] },
];

export interface HfModel {
  repo: string; up: string; labs?: boolean; base: string; name: string; fam: string;
  kind: "official" | "finetune"; unc: boolean; params: string; dl: number; likes: number; age: number;
  quants: Record<string, number>; tags: string;
}

export const MODELS: HfModel[] = [
  { repo: "LiquidAI/LFM3-1.7B-GGUF", up: "LiquidAI", labs: true, base: "lfm3-1.7b", name: "LFM3 1.7B", fam: "lf", kind: "official", unc: false, params: "1.7B", dl: 213189, likes: 230, age: 11, quants: { Q8_0: 1.9, Q6_K: 1.5, Q4_K_M: 1.1 }, tags: "tiny · fast · 256K context" },
  { repo: "unsloth/Qwen3.8-27B-GGUF", up: "unsloth", base: "qwen3.8-27b", name: "Qwen3.8 27B", fam: "qw", kind: "official", unc: false, params: "27B", dl: 1210000, likes: 910, age: 82, quants: { Q6_K: 22.4, Q5_K_M: 19.3, Q4_K_M: 16.5 }, tags: "reasoning · tools · 256K context" },
  { repo: "bartowski/Qwen_Qwen3.8-27B-GGUF", up: "bartowski", base: "qwen3.8-27b", name: "Qwen3.8 27B", fam: "qw", kind: "official", unc: false, params: "27B", dl: 640000, likes: 300, age: 81, quants: { Q6_K: 22.4, Q5_K_M: 19.3, Q4_K_M: 16.5 }, tags: "reasoning · tools · 256K context" },
  { repo: "unsloth/gemma-4-E4B-it-GGUF", up: "unsloth", base: "gemma-4-e4b", name: "Gemma 4 E4B", fam: "ge", kind: "official", unc: false, params: "7.5B", dl: 905000, likes: 520, age: 120, quants: { Q8_0: 8.0, Q6_K: 6.2, Q4_K_M: 4.6 }, tags: "sees images · 64K context" },
  { repo: "Qwen/Qwen3-VL-8B-Instruct-GGUF", up: "Qwen", labs: true, base: "qwen3-vl-8b", name: "Qwen3-VL 8B", fam: "qw", kind: "official", unc: false, params: "8B", dl: 602000, likes: 310, age: 400, quants: { Q8_0: 8.7, Q4_K_M: 5.0 }, tags: "sees images · 256K context" },
  { repo: "unsloth/Qwen3-14B-GGUF", up: "unsloth", base: "qwen3-14b", name: "Qwen3 14B", fam: "qw", kind: "official", unc: false, params: "14B", dl: 410000, likes: 380, age: 190, quants: { Q6_K: 12.1, Q5_K_M: 10.5, Q4_K_M: 9.0 }, tags: "reasoning · tools · 128K context" },
  { repo: "unsloth/Mistral-Small-3.2-24B-Instruct-2506-GGUF", up: "unsloth", base: "mistral-small-3.2", name: "Mistral Small 3.2", fam: "mi", kind: "official", unc: false, params: "24B", dl: 300000, likes: 450, age: 500, quants: { Q6_K: 19.3, Q5_K_M: 16.8, Q4_K_M: 14.3 }, tags: "sees images · tools · 128K context" },
  { repo: "bartowski/TheDrummer_Cydonia-24B-v4.2.0-GGUF", up: "bartowski", base: "cydonia-24b", name: "Cydonia 24B v4.2", fam: "mi", kind: "finetune", unc: false, params: "24B", dl: 121000, likes: 210, age: 300, quants: { Q6_K: 19.3, Q5_K_M: 16.8, Q4_K_M: 14.3 }, tags: "roleplay finetune of Mistral Small" },
  { repo: "TheDrummer/Cydonia-24B-v4.2.0-GGUF", up: "TheDrummer", base: "cydonia-24b", name: "Cydonia 24B v4.2", fam: "mi", kind: "finetune", unc: false, params: "24B", dl: 98000, likes: 260, age: 300, quants: { Q6_K: 19.3, Q5_K_M: 16.8, Q4_K_M: 14.3 }, tags: "roleplay finetune of Mistral Small" },
  { repo: "bartowski/Rocinante-12B-v1.1-GGUF", up: "bartowski", base: "rocinante-12b", name: "Rocinante 12B v1.1", fam: "mi", kind: "finetune", unc: false, params: "12B", dl: 88000, likes: 190, age: 800, quants: { Q6_K: 10.1, Q5_K_M: 8.7, Q4_K_M: 7.5 }, tags: "storytelling finetune of Mistral Nemo" },
  { repo: "bartowski/mlabonne_Qwen3-14B-abliterated-GGUF", up: "bartowski", base: "qwen3-14b-abl", name: "Qwen3 14B abliterated", fam: "qw", kind: "finetune", unc: true, params: "14B", dl: 150000, likes: 140, age: 180, quants: { Q6_K: 12.1, Q5_K_M: 10.5, Q4_K_M: 9.0 }, tags: "refusals removed" },
  { repo: "AtomicChat/LFM3-1.7B-Uncensored-GGUF", up: "AtomicChat", base: "lfm3-1.7b-unc", name: "LFM3 1.7B Uncensored", fam: "lf", kind: "finetune", unc: true, params: "1.7B", dl: 84188, likes: 258, age: 8, quants: { Q8_0: 1.9, Q4_K_M: 1.1 }, tags: "uncensored finetune" },
  { repo: "unsloth/DeepSeek-V4.1-Flash-GGUF", up: "unsloth", base: "deepseek-v4.1-flash", name: "DeepSeek V4.1 Flash", fam: "ll", kind: "official", unc: false, params: "197B", dl: 2650000, likes: 602, age: 12, quants: { Q4_K_M: 118, Q2_K: 64 }, tags: "mixture of experts · 1M context" },
];

export type DriveId = "C" | "E";

export interface LocalModel {
  id: number; name: string; fam: string; famName: string; type: string; tags: string; size: number;
  ctx: string; speed: string; drive: DriveId; used: string; fit: "ok" | "warn";
}

export const LOCAL: LocalModel[] = [
  { id: 1, name: "Mistral Small 3.2", fam: "mi", famName: "Mistral", type: "Text", tags: "24B · Q4_K_M · sees images · tools", size: 14.3, ctx: "128K", speed: "35 t/s", drive: "C", used: "Today", fit: "ok" },
  { id: 2, name: "Qwen3 32B", fam: "qw", famName: "Qwen", type: "Text", tags: "32B · Q4_K_M · reasoning", size: 19.8, ctx: "128K", speed: "9 t/s", drive: "E", used: "Oct 12", fit: "warn" },
  { id: 3, name: "Gemma 4 E4B", fam: "ge", famName: "Gemma", type: "Vision", tags: "7.5B · Q6_K · sees images", size: 6.2, ctx: "64K", speed: "est. 80 t/s", drive: "C", used: "Never", fit: "ok" },
  { id: 4, name: "Cydonia 24B v4.2", fam: "mi", famName: "Mistral", type: "Text", tags: "24B · Q4_K_M · roleplay finetune", size: 14.3, ctx: "128K", speed: "33 t/s", drive: "E", used: "Nov 1", fit: "ok" },
  { id: 5, name: "Qwen3-VL 8B", fam: "qw", famName: "Qwen", type: "Vision", tags: "8B · Q4_K_M · from MidSpec-Chatbot", size: 5.0, ctx: "256K", speed: "61 t/s", drive: "C", used: "Yesterday", fit: "ok" },
  { id: 6, name: "FLUX.2 Klein 4B", fam: "fl", famName: "FLUX", type: "Image", tags: "4B · Q8_0 · text to image", size: 4.4, ctx: "–", speed: "7 s / image", drive: "C", used: "Oct 30", fit: "ok" },
  { id: 7, name: "Kokoro", fam: "ko", famName: "Kokoro", type: "Speech", tags: "Q4 · voice output", size: 0.2, ctx: "–", speed: "real time", drive: "C", used: "Yesterday", fit: "ok" },
  { id: 8, name: "Whisper base.en", fam: "wh", famName: "Whisper", type: "Speech", tags: "q5_1 · voice input", size: 0.1, ctx: "–", speed: "real time", drive: "C", used: "Yesterday", fit: "ok" },
  { id: 9, name: "bge-m3", fam: "bg", famName: "BGE", type: "Embedding", tags: "Q8_0 · search and memory", size: 0.6, ctx: "8K", speed: "–", drive: "C", used: "Yesterday", fit: "ok" },
  { id: 10, name: "Llama 3.1 8B", fam: "ll", famName: "Llama", type: "Text", tags: "8B · Q6_K · tools", size: 6.6, ctx: "128K", speed: "72 t/s", drive: "E", used: "Aug 3", fit: "ok" },
];

export const DRIVES: Record<DriveId, { label: string; free: number; total: number }> = {
  C: { label: "C: internal SSD", free: 410, total: 1000 },
  E: { label: "E: external archive", free: 160, total: 2000 },
};

/** Placeholder config for sample entries; real configs come from templates and the library. */
export const sampleConfig = (label: string): Record<string, unknown> => ({ model_param: `${label}.gguf` });
