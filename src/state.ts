// App state. One mutable object; screens read it and call render() after changes.

import type { BinaryStatus, HardwareInfo, Machine, PopularResult, ServerStatus, Settings, Source as ApiSource, Template, UpdateInfo, LauncherUpdate } from "./api";
import type { DlState } from "./downloads";

export type Source = ApiSource;

export const S = {
  useGpu: [] as boolean[],
  /** Popular list: true = original models only (official + quants), false = with finetunes. */
  official: true,
  /** Family filter on the start screen; null = "Top" (best of each family). */
  family: null as string | null,
  /** Order of the popular list. */
  sort: "downloads" as import("./api").SortOrder,
  uncensored: false,
  showAllTiers: false,
  running: null as string | null,
  /** Which setup (download key) the running server came from: templates can share a title. */
  runningKey: null as string | null,
  /** Loaded from settings.json at start. */
  sources: [] as Source[],
  lib: {
    tab: "local" as "local" | "discover",
    q: "",
    types: new Set<string>(),
    fams: new Set<string>(),
    drives: new Set<string>(),
    confirm: null as string | null,
    /** Show only models that run on this computer. */
    runsOnly: false,
    /** Card order. */
    sort: "speed" as "speed" | "used" | "name" | "size",
  },
  adv: { page: "presets" },
  /** Launch config edited on the advanced page (a .kcpps as JSON). */
  cfg: {} as Record<string, unknown>,
  server: { state: "stopped" } as ServerStatus,
  logs: [] as string[],
  binary: null as BinaryStatus | null,
  update: null as UpdateInfo | null,
  /** This launcher's newest GitHub release (checked on start, like KoboldCpp). */
  selfUpdate: null as LauncherUpdate | null,
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

/** Graphics memory of the ticked GPUs (bytes). */
export const vram = () => (S.hw?.gpus ?? []).reduce((a, g, i) => a + (S.useGpu[i] !== false ? g.vram : 0), 0);

/** Share of RAM models may use; same values as `RAM_SHARE_*` in fit.rs. */
export const ramShare = () => (S.settings?.ramMode === "unstable" ? 0.98 : 0.6);
/** Whole bytes: Rust reads it as an integer (u64) and rejects fractions. */
export const ramForModels = () => Math.floor((S.hw?.ramTotal ?? 0) * ramShare());
/** What fit calculations need about this PC with the current GPU selection. */
export const machine = (context = 16384): Machine => ({ vram: vram(), ram: ramForModels(), unified: !!S.hw?.unifiedMemory && vram() > 0, context, speed: speedHw() });

/** Bandwidths of the selected GPUs: layers run one after another, so time adds up per share. */
function speedHw(): Machine["speed"] {
  const hw = S.hw;
  // Unknown values stay unknown (null): the estimate then says N/A instead of guessing.
  const cpu = hw?.ramBandwidth ?? null;
  const sel = (hw?.gpus ?? []).filter((_, i) => S.useGpu[i] !== false);
  const total = sel.reduce((a, g) => a + g.vram, 0);
  if (!hw || !total) return { gpuBandwidth: null, cpuBandwidth: cpu, apple: !!hw?.unifiedMemory };
  // Integrated GPUs use main memory, so theirs is the RAM bandwidth.
  const bws = sel.map((g) => g.bandwidth ?? (g.discrete ? null : cpu));
  if (bws.some((b) => !b)) return { gpuBandwidth: null, cpuBandwidth: cpu, apple: hw.unifiedMemory };
  const secPerByte = sel.reduce((a, g, i) => a + g.vram / total / bws[i]!, 0);
  return { gpuBandwidth: 1 / secPerByte, cpuBandwidth: cpu, apple: hw.unifiedMemory };
}

// Registered by main.ts so screens can trigger a redraw without import cycles.
let renderFn = () => {};
export const setRender = (fn: () => void) => (renderFn = fn);
export const render = () => renderFn();
