// Browser-preview data (only loaded outside the desktop app). Mirrors the Rust logic roughly so
// the UI can be checked in a normal browser; real numbers come from src-tauri.

import type { Fit, Machine, PopularItem, PopularResult, QuantPick, Ranked, RepoFiles, Source, Template } from "./api";
import { MODELS, TEMPLATES } from "./data";

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
    files: [["model_param", `https://huggingface.co/example/${t.id}.gguf`, t.size * GB]],
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

const toRanked = (x: (typeof MODELS)[number], source: number): Ranked => ({
  id: x.repo, name: x.name, author: x.up, params: x.params, official: x.kind === "official", uncensored: x.unc,
  likes: x.likes, downloads: x.dl, ageDays: x.age, source, baseModel: x.base, tags: x.tags.split(" · "),
  score: (Math.log10(x.dl + 1) + 0.6 * Math.log10(x.likes + 1)) * Math.max(0.35, 0.5 ** (x.age / 180)),
});

export function mockPopular(o: { officialOnly: boolean; showUncensored: boolean }, m: Machine, sources: Source[]): PopularResult {
  const on = sources.filter((s) => s.on && s.comps);
  let tooBig = 0;
  const best = new Map<string, PopularItem>();
  let kept = 0;
  for (const x of MODELS) {
    if (o.officialOnly && x.kind !== "official") continue;
    if (x.unc && !o.showUncensored) continue;
    const si = on.findIndex((s) => (s.id === "labs" ? !!x.labs : s.name === x.up) && (x.unc ? s.comps!.unc : x.kind === "official" ? s.comps!.official : s.comps!.finetune));
    if (si < 0) continue;
    const p = pick(x.quants, m);
    if (!p) { tooBig++; continue; }
    kept++;
    const item: PopularItem = { ...toRanked(x, si), pick: p, urls: [`https://huggingface.co/${x.repo}/resolve/main/model-${p.quant}.gguf`], mmproj: null };
    const cur = best.get(x.base);
    if (!cur || si < cur.source) best.set(x.base, item);
  }
  const items = [...best.values()].sort((a, b) => b.score - a.score);
  return { items, tooBig, dupes: kept - items.length, errors: [] };
}

export function mockSearch(q: string): Ranked[] {
  const l = q.toLowerCase();
  return MODELS.filter((x) => (x.name + x.repo).toLowerCase().includes(l)).map((x) => toRanked(x, 0));
}

export function mockRepoFiles(repo: string): RepoFiles {
  const x = MODELS.find((m) => m.repo === repo);
  return {
    quants: Object.entries(x?.quants ?? { Q4_K_M: 4.6 }).map(([q, s]) => ({ quant: q, label: q, files: [`model-${q}.gguf`], size: s * GB })),
    mmproj: x?.tags.includes("sees images") ? [["mmproj-F16.gguf", 0.9 * GB]] : [],
  };
}
