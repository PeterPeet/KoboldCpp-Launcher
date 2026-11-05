// Browser-preview data (only loaded outside the desktop app). Mirrors the Rust logic roughly so
// the UI can be checked in a normal browser; real numbers come from src-tauri.

import type { Fit, LibraryView, LocalModel, Machine, ModelKind, PopularItem, PopularResult, QuantPick, Ranked, RepoFiles, Source, Template } from "./api";
import { DRIVES, LOCAL, MODELS, TEMPLATES } from "./data";

const GB = 1024 ** 3;

export function mockFit(size: number, context: number, m: Machine): Fit {
  const need = size + 0.5 * GB + Math.min(Math.max(size * 0.03, 0.12 * GB), 0.6 * GB) * (context / 4096);
  if (m.unified) return need <= m.vram ? "full" : "none";
  if (m.vram === 0) return size <= m.ram * 0.6 ? "cpu" : "none";
  if (need <= m.vram) return "full";
  return size <= m.vram + m.ram * 0.5 ? "partial" : "none";
}

export function mockTemplates(): Template[] {
  return TEMPLATES.map((t) => ({
    id: t.id, collection: "newbie-templates", url: `https://huggingface.co/koboldcpp/newbie-templates/resolve/main/${t.id}.kcppt`,
    tier: t.tier, title: t.use, parts: t.parts, size: t.size * GB, context: t.ctx ? 24576 : 8192,
    config: { model_param: `https://huggingface.co/example/${t.id}.gguf`, contextsize: 8192, istemplate: true },
    files: [["model_param", `https://huggingface.co/example/${t.id}.gguf`, t.size * GB]], downloaded: t.id === "MidSpec-Chatbot",
  }));
}

function pick(quants: Record<string, number>, m: Machine): QuantPick | null {
  for (const q of ["Q6_K", "Q5_K_M", "Q4_K_M"]) {
    const s = quants[q] * GB;
    if (quants[q] != null && ["full", "cpu"].includes(mockFit(s, m.context, m))) return { quant: q, size: s, fit: mockFit(s, m.context, m), fallback: false };
  }
  const names = Object.keys(quants);
  const q = quants.Q4_K_M != null ? "Q4_K_M" : names[names.length - 1];
  const s = quants[q] * GB;
  const f = mockFit(s, m.context, m);
  return f === "none" ? null : { quant: q, size: s, fit: f, fallback: q !== "Q4_K_M" };
}

const famOf = (n: string) => ["gemma", "qwen", "glm", "deepseek", "mistral", "llama", "nemotron"].find((f) => n.toLowerCase().includes(f) || (f === "mistral" && /cydonia|rocinante/i.test(n))) ?? null;
const toRanked = (x: (typeof MODELS)[number], source: number): Ranked => ({
  id: x.repo, name: x.name, author: x.up, params: x.params, uncensored: x.unc,
  kind: x.kind === "finetune" ? "finetune" : x.labs ? "official" : "quant", official: !!x.labs, family: famOf(x.repo),
  likes: x.likes, downloads: x.dl, ageDays: x.age, source, baseModel: x.base, tags: x.tags.split(" · "),
  score: (Math.log10(x.dl + 1) + 0.6 * Math.log10(x.likes + 1)) * Math.max(0.35, 0.5 ** (x.age / 180)),
});

export function mockPopular(o: { officialOnly: boolean; showUncensored: boolean; sort: string }, m: Machine, sources: Source[], family: string | null): PopularResult {
  const on = sources.filter((s) => s.on && s.comps);
  let tooBig = 0;
  const best = new Map<string, PopularItem>();
  let kept = 0;
  for (const x of MODELS) {
    if (o.officialOnly && x.kind !== "official") continue;
    if (!family && !x.labs && x.kind === "finetune") continue;
    if (x.unc && !o.showUncensored) continue;
    const si = on.findIndex((s) => (s.id === "labs" ? !!x.labs : s.name === x.up) && (x.unc ? s.comps!.unc : x.kind === "official" ? s.comps!.official : s.comps!.finetune));
    if (si < 0) continue;
    const p = pick(x.quants, m);
    if (!p) { tooBig++; continue; }
    kept++;
    const item: PopularItem = { ...toRanked(x, si), tps: mockTps(p.size, m), measured: null, pick: p, urls: [`https://huggingface.co/${x.repo}/resolve/main/model-${p.quant}.gguf`], mmproj: x.tags.includes("sees images") ? [`https://huggingface.co/${x.repo}/resolve/main/mmproj-F16.gguf`, 0.9 * GB] : null, downloaded: x.repo.includes("LFM3-1.7B-GGUF"), modelDownloaded: x.repo.includes("LFM3-1.7B-GGUF") || x.repo.includes("Qwen3-VL"), mmprojDownloaded: false, mtp: x.repo.includes("Qwen3.8") };
    const cur = best.get(x.base);
    if (!cur || si < cur.source) best.set(x.base, item);
  }
  const key = (i: PopularItem) => (o.sort === "likes" ? i.likes : o.sort === "newest" ? -i.ageDays : o.sort === "trending" ? i.score : i.downloads);
  let items = [...best.values()].sort((a, b) => key(b) - key(a));
  if (family === "__all") { /* search: everything */ }
  else if (family) items = items.filter((i) => i.family === family);
  else {
    const seen = new Set<string>();
    items = items.filter((i) => i.family && !seen.has(i.family) && seen.add(i.family));
  }
  return { items, tooBig, dupes: kept - items.length, errors: [] };
}

export function mockSearch(q: string, m: Machine, sources: Source[]): PopularResult {
  const l = q.toLowerCase();
  const all = mockPopular({ officialOnly: false, showUncensored: true, sort: "downloads" }, m, sources.map((s) => ({ ...s, on: true })), "__all");
  return { ...all, items: all.items.filter((i) => (i.name + i.id).toLowerCase().includes(l)) };
}

/** Same idea as fit::estimate_tps, roughly. */
export function mockTps(size: number, m: Machine) {
  const gb = size / GB;
  const bw = m.speed.gpuBandwidth ?? (m.speed.cpuBandwidth ? m.speed.cpuBandwidth * 0.7 : null);
  if (!bw) return null;
  return 1 / (gb / (bw * (m.speed.apple ? 0.6 : 0.7)) + (m.speed.apple ? 0.006 : 0.002));
}

export function mockRepoFiles(repo: string): RepoFiles {
  const x = MODELS.find((m) => m.repo === repo);
  return {
    quants: Object.entries(x?.quants ?? { Q4_K_M: 4.6 }).map(([q, s]) => ({ quant: q, label: q, files: [`model-${q}.gguf`], size: s * GB })),
    mmproj: x?.tags.includes("sees images") ? [["mmproj-F16.gguf", 0.9 * GB]] : [],
    mtp: !!x?.repo.includes("Qwen3.8"),
  };
}

let removed = new Set<string>();
export function mockTrash(path: string) { removed.add(path); }

export function mockLibrary(): LibraryView {
  const kind = (t: string): ModelKind => (t === "Image" || t === "Speech" || t === "Embedding" || t === "Vision" ? t : "Text");
  const models: LocalModel[] = LOCAL.map((m) => ({
    path: `/${m.drive}/models/${m.name.replace(/ /g, "-")}.gguf`, file: `${m.name.replace(/ /g, "-")}.gguf`, name: m.name, kind: kind(m.type),
    architecture: m.famName.toLowerCase(), quant: m.tags.split(" · ")[1] ?? null, sizeLabel: m.tags.split(" · ")[0], size: m.size * GB, parts: m.id === 4 ? 3 : 1,
    layers: 40, contextLength: m.ctx === "–" ? null : parseInt(m.ctx) * 1024, headCountKv: 8, kvLen: 128,
    mmproj: m.type === "Vision" ? `/${m.drive}/models/mmproj-F16.gguf` : null, drive: `/${m.drive}`, modified: null,
    lastUsed: m.used === "Never" ? null : Date.now() / 1000 - 86400 * (m.id % 9),
    missingParts: m.id === 4 ? 1 : 0, readable: true, format: "gguf" as const, mtp: m.id === 2,
    support: m.type === "Image" || m.type === "Speech" ? null : m.id === 10 ? "no" as const : "yes" as const,
  })).filter((m) => !removed.has(m.path));
  models.push({
    path: "/C/lmstudio/gemma-4-E2B-it-MLX-4bit", file: "gemma-4-E2B-it-MLX-4bit", name: "gemma-4-E2B-it-MLX-4bit", kind: "Vision", architecture: "gemma4",
    quant: "4-bit", sizeLabel: null, size: 4.3 * GB, parts: 1, layers: 35, contextLength: 131072, headCountKv: null, kvLen: null, mmproj: null,
    drive: "/C", modified: null, lastUsed: null, missingParts: 0, readable: true, support: null, format: "mlx", mtp: false,
  });
  return {
    models,
    drives: Object.entries(DRIVES).map(([k, d]) => ({ mount: `/${k}`, name: d.label, total: d.total * GB, free: d.free * GB, removable: k === "E" })),
    folders: [{ path: "/C/models", recursive: true }, { path: "/E/models", recursive: false }], modelsDir: "/C/models",
  };
}
