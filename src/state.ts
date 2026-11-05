// App state. One mutable object; screens read it and call render() after changes.

import type { BinaryStatus, HardwareInfo, Machine, PopularResult, ServerStatus, Settings, Source as ApiSource, Template, UpdateInfo } from "./api";
import type { DlState } from "./downloads";

export type Source = ApiSource;

export const S = {
  useGpu: [] as boolean[],
  /** Popular list: true = original models only (official + quants), false = with finetunes. */
  official: true,
  /** Family filter on the start screen; null = "Top" (best of each family). */
  family: null as string | null,
  uncensored: false,
  showAllTiers: false,
  running: null as string | null,
  /** Loaded from settings.json at start. */
  sources: [] as Source[],
  lib: {
    tab: "local" as "local" | "discover",
    q: "",
    types: new Set<string>(),
    fams: new Set<string>(),
    drives: new Set<string>(),
    confirm: null as string | null,
  },
  adv: { page: "hardware" },
  /** Launch config edited on the advanced page (a .kcpps as JSON). */
  cfg: {} as Record<string, unknown>,
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

/** Graphics memory of the ticked GPUs (bytes). */
export const vram = () => (S.hw?.gpus ?? []).reduce((a, g, i) => a + (S.useGpu[i] !== false ? g.vram : 0), 0);

/** What fit calculations need about this PC with the current GPU selection. */
export const machine = (context = 16384): Machine => ({ vram: vram(), ram: S.hw?.ramTotal ?? 0, unified: !!S.hw?.unifiedMemory && vram() > 0, context });

// Registered by main.ts so screens can trigger a redraw without import cycles.
let renderFn = () => {};
export const setRender = (fn: () => void) => (renderFn = fn);
export const render = () => renderFn();
