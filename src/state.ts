// App state. One mutable object; screens read it and call render() after changes.

import type { BinaryStatus, HardwareInfo, Machine, PopularResult, ServerStatus, Settings, Source as ApiSource, Template, UpdateInfo } from "./api";
import type { DlState } from "./downloads";
import { TIERS, type TierId } from "./data";

export type Source = ApiSource;

export const S = {
  tier: "mid" as TierId,
  useGpu: [] as boolean[],
  official: true,
  uncensored: false,
  showAllTiers: false,
  running: null as string | null,
  downloads: {} as Record<string, { pct: number; size: number }>,
  /** Loaded from settings.json at start. */
  sources: [] as Source[],
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
  server: { state: "stopped" } as ServerStatus,
  logs: [] as string[],
  binary: null as BinaryStatus | null,
  update: null as UpdateInfo | null,
  updateError: "",
  settings: null as Settings | null,
  hw: null as HardwareInfo | null,
  dl: {} as Record<string, DlState>,
  templates: null as Template[] | null,
  templatesError: "",
  popular: null as PopularResult | null,
  popularKey: "",
  popularError: "",
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

/** Graphics memory of the ticked GPUs (bytes). */
export const vram = () => (S.hw?.gpus ?? []).reduce((a, g, i) => a + (S.useGpu[i] !== false ? g.vram : 0), 0);

/** What fit calculations need about this PC with the current GPU selection. */
export const machine = (context = 16384): Machine => ({ vram: vram(), ram: S.hw?.ramTotal ?? 0, unified: !!S.hw?.unifiedMemory && vram() > 0, context });

// Registered by main.ts so screens can trigger a redraw without import cycles.
let renderFn = () => {};
export const setRender = (fn: () => void) => (renderFn = fn);
export const render = () => renderFn();
