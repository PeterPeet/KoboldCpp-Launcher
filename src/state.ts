// App state. One mutable object; screens read it and call render() after changes.

import { TIERS, type TierId } from "./data";

export interface Source {
  id: string; name: string; builtin?: boolean; desc: string; on: boolean;
  comps: { official: boolean; finetune: boolean; unc: boolean } | null;
}

export const S = {
  tier: "mid" as TierId,
  useGpu: [] as boolean[],
  official: true,
  uncensored: false,
  showAllTiers: false,
  running: null as string | null,
  downloads: {} as Record<string, { pct: number; size: number }>,
  sources: [
    { id: "kcppt", name: "KoboldCpp templates", builtin: true, desc: "Ready-made setups from hf.co/koboldcpp", on: true, comps: null },
    { id: "labs", name: "Labs' own uploads", builtin: true, desc: "Repo author is the base model author", on: true, comps: { official: true, finetune: false, unc: false } },
    { id: "unsloth", name: "unsloth", desc: "393 GGUF repos", on: true, comps: { official: true, finetune: false, unc: false } },
    { id: "bartowski", name: "bartowski", desc: "1,000+ GGUF repos", on: false, comps: { official: true, finetune: true, unc: false } },
    { id: "TheDrummer", name: "TheDrummer", desc: "106 GGUF repos", on: false, comps: { official: false, finetune: true, unc: false } },
  ] as Source[],
  lib: {
    tab: "local" as "local" | "discover",
    q: "",
    types: new Set<string>(),
    fams: new Set<string>(),
    drives: new Set<string>(),
    confirm: null as number | null,
  },
  adv: {
    page: "hardware",
    backend: "cuda",
    gpus: [true, true],
    split: 60,
    layers: 65,
    ctx: 16384,
    fa: true,
    kv: "f16",
    main: 0,
    img: 1,
    mode: "layer",
  },
  upd: {
    ver: "v1.123.1",
    latest: "v1.124.2",
    date: "Oct 31, 2026",
    onStart: true,
    pre: false,
    state: "idle" as "idle" | "working",
    pct: 0,
    step: "",
  },
};

try {
  const t = localStorage.getItem("kl-tier");
  if (t && t in TIERS) S.tier = t as TierId;
} catch {
  /* storage unavailable: keep default */
}
S.useGpu = TIERS[S.tier].gpus.map(() => true);

export function saveTier() {
  try {
    localStorage.setItem("kl-tier", S.tier);
  } catch {
    /* ignore */
  }
}

/** Graphics memory of the ticked GPUs. */
export const vram = () => TIERS[S.tier].gpus.reduce((a, g, i) => a + (S.useGpu[i] ? g.vram : 0), 0);

// Registered by main.ts so screens can trigger a redraw without import cycles.
let renderFn = () => {};
export const setRender = (fn: () => void) => (renderFn = fn);
export const render = () => renderFn();
