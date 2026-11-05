// Typed wrappers around the Rust commands. Outside Tauri (browser preview) the calls fall back
// to sample data so the UI can still be checked in a normal browser.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/* ---------- types (mirror src-tauri) ---------- */

export type Backend =
  | { kind: "auto" }
  | { kind: "cuda"; gpu: number | null; lowvram: boolean; mmq: boolean }
  | { kind: "vulkan"; gpus: number[] }
  | { kind: "cpu" };

export interface Gpu { index: number; name: string; api: "cuda" | "rocm" | "vulkan" | "metal"; vram: number; vramFree: number | null; discrete: boolean; bandwidth: number | null }
export interface HardwareInfo {
  os: string; arch: string; cpuName: string; logicalCpus: number; defaultThreads: number; avx2: boolean | null;
  ramTotal: number; ramAvailable: number; unifiedMemory: boolean; gpus: Gpu[]; ramBandwidth: number | null; ramType: string | null; suggestedBackend: Backend; tier: "low" | "mid" | "high";
}
export interface InstalledBinary { path: string; version: string | null; asset: string | null }
export interface BinaryStatus { installed: InstalledBinary | null; present: boolean; asset: string | null }
export interface ReleaseInfo { tag: string; publishedAt: string | null; url: string | null; notes: string; asset: string | null; size: number | null }
export interface LauncherUpdate { current: string; latest: string | null; url: string | null; updateAvailable: boolean }
export interface UpdateInfo { current: string | null; latest: ReleaseInfo | null; updateAvailable: boolean }
export interface Progress { done: number; total: number | null; speed: number }
/** One launch option from KoboldCpp's (or Esobold's) option table. */
export interface ArgSpec { dest: string; flags: string[]; group: string; kind: "flag" | "value" | "list" | "optional"; type: string; default: unknown; choices?: unknown[] | null; min?: number | null; max?: number | null; help?: string | null }
export interface Settings {
  binary: InstalledBinary | null; checkUpdatesOnStart: boolean; includePrereleases: boolean;
  /** Esobold mode (Advanced setup › Esobold) and its own installed program. */
  esobold: boolean; esoboldBinary: InstalledBinary | null;
  modelsDir: string | null; libraryFolders: LibraryFolder[]; sources: Source[];
}
export interface Paths { data: string; models: string; presets: string; logs: string }
export interface Logs { dir: string; koboldcpp: string; launcher: string }
export type ServerStatus =
  | { state: "stopped" }
  | { state: "starting"; port: number }
  | { state: "running"; port: number }
  | { state: "failed"; message: string };
export type ServerEvent = { type: "status"; status: ServerStatus } | { type: "log"; line: string };
export type Kcpps = Record<string, unknown>;
export type Fit = "full" | "partial" | "cpu" | "none";
export interface QuantPick { quant: string; size: number; fit: Fit; fallback: boolean }
export interface Template {
  id: string; collection: string; url: string; tier: "low" | "mid" | "high" | null; title: string; parts: string[];
  size: number | null; context: number | null; config: Kcpps; files: [string, string, number | null][];
  /** All files already on this computer. */
  downloaded: boolean;
}
export type ModelKindHf = "official" | "quant" | "finetune";
export interface Ranked {
  id: string; name: string; author: string; params: string | null; kind: ModelKindHf; official: boolean; family: string | null; uncensored: boolean;
  likes: number; downloads: number; ageDays: number; score: number; source: number; baseModel: string | null; tags: string[];
}
export interface SettingsPatch { checkUpdatesOnStart?: boolean; includePrereleases?: boolean; sources?: Source[] }
export interface PopularItem extends Ranked {
  pick: QuantPick; urls: string[]; mmproj: [string, number] | null;
  /** Everything (model + vision file) is on this computer. */
  downloaded: boolean; modelDownloaded: boolean; mmprojDownloaded: boolean;
  /** How the model can use MTP (faster answers); null: not possible or unknown. */
  mtp: Mtp | null;
  /** The MTP head file is on this computer (false when there is none to download). */
  mtpDownloaded: boolean;
  /** Estimated tokens/s here; `measured` when KoboldCpp already ran this file here. */
  tps: number | null; measured: number | null;
}
export type SortOrder = "trending" | "downloads" | "likes" | "newest";
/** hidden (search only): matches whose uploader's source is off, per uploader. */
export interface PopularResult { items: PopularItem[]; tooBig: number; dupes: number; errors: string[]; hidden?: [string, number][] }
export interface QuantFile { quant: string; label: string; files: string[]; size: number }
/** builtIn: the model file has MTP layers (--usemtp). Otherwise a separate MTP head file (--draftmodel). */
export interface Mtp { builtIn: boolean; url: string | null; size: number | null }
export interface RepoFiles { quants: QuantFile[]; mmproj: [string, number][]; mtp: [string, number][] }
export interface DlProgress { id: string; done: number; total: number | null; speed: number; file: string }
export interface Source {
  id: string; name: string; builtin: boolean; on: boolean; desc: string;
  comps: { official: boolean; finetune: boolean; unc: boolean } | null;
}
export interface Machine {
  vram: number; ram: number; unified: boolean; context: number;
  /** Memory bandwidths (GB/s) for the speed estimate. */
  speed: { gpuBandwidth: number | null; cpuBandwidth: number | null; apple: boolean };
}
export type ModelKind = "Text" | "Vision" | "Image" | "Speech" | "Embedding" | "Projector" | "MtpHead";
export interface LocalModel {
  path: string; file: string; name: string; kind: ModelKind; architecture: string | null; quant: string | null; sizeLabel: string | null;
  size: number; parts: number; layers: number | null; contextLength: number | null; headCountKv: number | null; kvLen: number | null;
  mmproj: string | null; drive: string | null; modified: number | null; lastUsed: number | null;
  missingParts: number; readable: boolean; support: "yes" | "no" | "unknown" | null;
  /** "gguf" runs in KoboldCpp; "mlx" / "safetensors" folders are listed but can't be loaded. */
  format: "gguf" | "mlx" | "safetensors";
  /** The file has MTP layers (faster answers with --usemtp). */
  mtp: boolean;
  /** No MTP layers of its own, but an MTP head next to it (--draftmodel): path and size. */
  mtpDraft: [string, number] | null;
}
export interface LocalFit { fit: Fit; gpuLayers: number; memory: number; tps: number | null; measured: number | null }
export interface Drive { mount: string; name: string; total: number; free: number; removable: boolean }
export interface LibraryFolder { path: string; recursive: boolean }
export interface LibraryView { models: LocalModel[]; drives: Drive[]; folders: LibraryFolder[]; modelsDir: string }
export interface PresetInfo { name: string; path: string; modified: number | null }

/* ---------- plumbing ---------- */

/** README screenshots: with `?fixture` the browser preview answers from data recorded on a real
 *  computer (`src-tauri/tests/screenshot_data.rs`) instead of its sample data. */
export const fixtureMode = !inTauri && typeof location !== "undefined" && new URLSearchParams(location.search).has("fixture");
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Rec = Record<string, any>;
let fixture: Promise<Rec> | null = null;
function fromFixture(f: Rec, cmd: string, args: Record<string, unknown>): unknown {
  switch (cmd) {
    case "popular_models": return f.popular_models?.[(args.family as string | null) ?? ""];
    case "search_models": return f.search_models?.[String(args.query).trim().toLowerCase()];
    case "fits": return (args.items as [number, number][]).map(([s, c]) => f.fits?.[`${s}:${c}`]);
    case "fit_local": return (args.items as { path: string }[]).map((i) => f.fit_local?.[i.path]);
    default: return f[cmd];
  }
}

async function call<T>(cmd: string, args: Record<string, unknown> = {}, mock?: () => T | Promise<T>): Promise<T> {
  if (!inTauri) {
    if (fixtureMode) {
      const f = await (fixture ??= fetch("/shots-data/fixture.json", { cache: "no-store" }).then((r) => r.json()));
      const v = fromFixture(f, cmd, args);
      if (v !== undefined && !(Array.isArray(v) && v.includes(undefined))) return v as T;
    }
    if (mock) return mock();
    throw new Error("Only available in the desktop app");
  }
  return invoke<T>(cmd, args);
}

/** Subscribe to a backend event; returns an unsubscribe function. */
export function on<T>(event: string, cb: (payload: T) => void): () => void {
  if (!inTauri) {
    mockListeners.push({ event, cb: cb as (p: unknown) => void });
    return () => {};
  }
  let un: (() => void) | undefined;
  listen<T>(event, (e) => cb(e.payload)).then((u) => (un = u));
  return () => un?.();
}

/* ---------- browser-preview stand-ins ---------- */

const GB = 1024 ** 3;
const mockListeners: { event: string; cb: (p: unknown) => void }[] = [];
const emitMock = (event: string, payload: unknown) => mockListeners.filter((l) => l.event === event).forEach((l) => l.cb(payload));
let mockSettings: Settings = {
  binary: { path: "/Applications/koboldcpp", version: "v1.123.1", asset: "koboldcpp-mac-arm64" }, checkUpdatesOnStart: true, includePrereleases: false, modelsDir: null, libraryFolders: [],
  esobold: false, esoboldBinary: null,
  sources: [
    { id: "kcppt", name: "KoboldCpp templates", builtin: true, desc: "Ready-made setups from hf.co/koboldcpp", on: true, comps: null },
    ...([["google", "Google", "Gemma"], ["Qwen", "Qwen (Alibaba)", "Qwen"], ["zai-org", "Z.ai", "GLM"], ["deepseek-ai", "DeepSeek", "DeepSeek"], ["mistralai", "Mistral AI", "Mistral"], ["meta-llama", "Meta", "Llama"], ["openai", "OpenAI", "GPT-oss"], ["nvidia", "NVIDIA", "Nemotron"]] as const).map(([id, maker, fam]) => (
      { id, name: id, builtin: true, desc: `${maker}: official ${fam} uploads (hf.co/${id})`, on: true, comps: { official: true, finetune: false, unc: false } })),
    { id: "unsloth", name: "unsloth", builtin: false, desc: "Official quants of new models", on: true, comps: { official: true, finetune: false, unc: false } },
    { id: "bartowski", name: "bartowski", builtin: false, desc: "Official quants and community finetunes", on: false, comps: { official: true, finetune: true, unc: false } },
    { id: "TheDrummer", name: "TheDrummer", builtin: false, desc: "Roleplay and story finetunes", on: false, comps: { official: false, finetune: true, unc: false } },
  ],
};
let mockServer: ServerStatus = { state: "stopped" };
const setMockServer = (s: ServerStatus) => { mockServer = s; emitMock("server", { type: "status", status: s }); };

const mockHardware: HardwareInfo = {
  os: "windows", arch: "x86_64", cpuName: "AMD Ryzen 9 7950X", logicalCpus: 32, defaultThreads: 15, avx2: true,
  ramTotal: 64 * GB, ramAvailable: 48 * GB, unifiedMemory: false,
  gpus: [
    { index: 0, name: "NVIDIA GeForce RTX 4090", api: "cuda", vram: 24 * GB, vramFree: 23 * GB, discrete: true, bandwidth: 1008 },
    { index: 1, name: "NVIDIA GeForce RTX 3060", api: "cuda", vram: 12 * GB, vramFree: 11.5 * GB, discrete: true, bandwidth: 360 },
  ],
  ramBandwidth: 100,
  ramType: null,
  suggestedBackend: { kind: "cuda", gpu: null, lowvram: false, mmq: true }, tier: "high",
};

/* ---------- commands ---------- */

export const api = {
  hardware: (refresh = false) => call<HardwareInfo>("hardware_info", { refresh }, () => mockHardware),
  settings: () => call<Settings>("get_settings", {}, () => mockSettings),
  saveSettings: (patch: SettingsPatch) => call<Settings>("save_settings", { patch }, () => (mockSettings = { ...mockSettings, ...patch })),
  paths: () => call<Paths>("app_paths", {}, () => ({ data: "~/KoboldCpp Launcher", models: "~/KoboldCpp Launcher/models", presets: "~/KoboldCpp Launcher/presets", logs: "~/KoboldCpp Launcher/logs" })),
  readLogs: () => call<Logs>("read_logs", {}, () => ({ dir: "~/KoboldCpp Launcher/logs", koboldcpp: "***\nWelcome to KoboldCpp - Version 1.124.2\nLoad Text Model OK: True\nPlease connect to custom endpoint at http://localhost:5001\n", launcher: "2026-11-05 03:47:01 UTC  launcher 0.1.0 started\n2026-11-05 03:47:20 UTC  start koboldcpp (model Qwen3-1.7B-Q8_0.gguf, port 5001)\n2026-11-05 03:47:31 UTC  KoboldCpp: Running { port: 5001 }\n" })),
  commandLine: (config: Kcpps) => call<string>("command_line", { config }),

  binaryStatus: () => call<BinaryStatus>("binary_status", {}, () => ({ installed: mockSettings.binary, present: true, asset: "koboldcpp.exe" })),
  checkUpdate: () =>
    call<UpdateInfo>("check_update", {}, () => ({
      current: mockSettings.binary?.version ?? null,
      latest: { tag: "v1.124.2", publishedAt: "2026-10-31T12:00:00Z", url: "https://github.com/LostRuins/koboldcpp/releases/tag/v1.124.2", notes: "Release notes from GitHub appear here (sample text in the browser preview).", asset: "koboldcpp.exe", size: 622841516 },
      updateAvailable: mockSettings.binary?.version !== "v1.124.2",
    })),
  launcherUpdate: () => call<LauncherUpdate>("launcher_update", {}, () => ({ current: "v0.1.0", latest: "v0.1.0", url: null, updateAvailable: false })),
  installBinary: (tag?: string) =>
    call<InstalledBinary>("install_binary", { tag: tag ?? null }, async () => {
      for (let p = 0; p <= 100; p += 10) {
        emitMock("binary-progress", { done: p * 6e6, total: 600e6, speed: 40e6 });
        await new Promise((r) => setTimeout(r, 120));
      }
      mockSettings = { ...mockSettings, binary: { path: "/x/koboldcpp", version: tag ?? "v1.124.2", asset: "koboldcpp.exe" } };
      return mockSettings.binary!;
    }),
  cancelInstall: () => call<void>("cancel_binary_install", {}, () => {}),
  /** Esobold's own launch options (offered in Esobold mode). */
  esoboldOptions: () => call<ArgSpec[]>("esobold_options", {}, () => [
    { dest: "jinja_tools", flags: ["--jinja_tools"], group: "Advanced Commands", kind: "flag", type: "bool", default: false, help: "Enables using jinja chat template formatting for chat completions endpoint." },
    { dest: "fsmaxsize", flags: ["--fsmaxsize"], group: "Advanced Commands", kind: "value", type: "int", default: 0, help: "Enable managed filesystem endpoints when > 0." },
    { dest: "fsdir", flags: ["--fsdir"], group: "Advanced Commands", kind: "value", type: "str", default: "", help: "Filesystem root directory." },
    { dest: "OpenLumara", flags: ["--OpenLumara"], group: "OpenLumara Commands", kind: "flag", type: "bool", default: false, help: "Enable and launch the OpenLumara AI agent alongside KoboldCpp." },
  ]),
  setEsobold: (on: boolean) => call<Settings>("set_esobold", { on }, () => (mockSettings = { ...mockSettings, esobold: on })),
  setBinaryPath: (path: string) => call<InstalledBinary>("set_binary_path", { path }),

  startServer: (config: Kcpps) =>
    call<number>("start_server", { config }, () => {
      setMockServer({ state: "starting", port: 5001 });
      setTimeout(() => setMockServer({ state: "running", port: 5001 }), 1200);
      return 5001;
    }),
  stopServer: () => call<void>("stop_server", {}, () => setMockServer({ state: "stopped" })),
  serverStatus: () => call<ServerStatus>("server_status", {}, () => mockServer),
  serverLogs: (last = 500) => call<string[]>("server_logs", { last }, () => ["(browser preview: no KoboldCpp output)"]),
  serverPerf: () => call<Record<string, unknown>>("server_perf"),

  templates: () => call<Template[]>("templates", {}, () => import("./mock").then((m) => m.mockTemplates())),
  fits: (items: [number, number][], machine: Machine) =>
    call<Fit[]>("fits", { items, vram: machine.vram, ram: machine.ram, unified: machine.unified }, () => import("./mock").then((m) => items.map(([s, c]) => m.mockFit(s, c, machine)))),
  popular: (options: { officialOnly: boolean; showUncensored: boolean; sort: SortOrder }, machine: Machine, family: string | null) =>
    call<PopularResult>("popular_models", { options, machine, family }, () => import("./mock").then((m) => m.mockPopular(options, machine, mockSettings.sources, family))),
  families: () => call<[string, string, string][]>("model_families", {}, () => [["gemma", "Gemma", "google"], ["qwen", "Qwen", "Qwen"], ["glm", "GLM", "zai-org"], ["deepseek", "DeepSeek", "deepseek-ai"], ["mistral", "Mistral", "mistralai"], ["llama", "Llama", "meta-llama"], ["gpt-oss", "GPT-oss", "openai"], ["nemotron", "Nemotron", "nvidia"]]),
  search: (query: string, machine: Machine) =>
    call<PopularResult>("search_models", { query, machine }, () => import("./mock").then((m) => m.mockSearch(query, machine, mockSettings.sources))),
  repoFiles: (repo: string) => call<RepoFiles>("repo_files", { repo }, () => import("./mock").then((m) => m.mockRepoFiles(repo))),
  downloadFiles: (id: string, files: { url: string; size: number | null }[]) =>
    call<[string, string][]>("download_files", { id, files }, async () => {
      const total = files.reduce((a, f) => a + (f.size ?? 1e9), 0);
      for (let p = 0; p <= 100; p += 5) {
        if (mockCancelled.delete(id)) throw new Error("download cancelled");
        emitMock("download-progress", { id, done: (total * p) / 100, total, speed: 60e6, file: "model.gguf" });
        await new Promise((r) => setTimeout(r, 120));
      }
      return files.map((f) => [f.url, "/models/" + f.url.split("/").pop()] as [string, string]);
    }),
  cancelDownload: (id: string) => call<void>("cancel_download", { id }, () => void mockCancelled.add(id)),
  exportSources: (path: string) => call<void>("export_sources", { path }),

  fitLocal: (items: { size: number; layers: number | null; headCountKv: number | null; kvLen: number | null; mmproj: number | null; path: string; name: string }[], machine: Machine) =>
    call<LocalFit[]>("fit_local", { items, machine }, () => import("./mock").then((m) => items.map((i) => ({
      fit: m.mockFit(i.size + (i.mmproj ?? 0), machine.context, machine), gpuLayers: (i.layers ?? 0) + 1,
      memory: i.size * 1.15 + (i.mmproj ?? 0) + 1.2 * 1024 ** 3, tps: m.mockTps(i.size, machine), measured: i.name.includes("Mistral") ? 31.4 : null })))),
  library: () => call<LibraryView>("library_scan", {}, () => import("./mock").then((m) => m.mockLibrary())),
  addFolder: (path: string, recursive = true) => call<Settings>("add_library_folder", { path, recursive }, () => (mockSettings = { ...mockSettings, libraryFolders: [...mockSettings.libraryFolders, { path, recursive }] })),
  previewFolder: (path: string, recursive: boolean) =>
    call<{ models: number; folders: number }>("preview_folder", { path, recursive }, () => ({ models: recursive ? 3 : 0, folders: recursive ? 2 : 0 })),
  removeFolder: (path: string) => call<Settings>("remove_library_folder", { path }),
  moveModel: (path: string, dest: string) => call<string>("move_model", { path, dest }),
  trashModel: (path: string) => call<void>("trash_model", { path }, () => import("./mock").then((m) => m.mockTrash(path))),
  splitUsage: (size: number, context: number, shares: number[], mainGpu: number) =>
    call<number[]>("split_usage", { size, context, shares, mainGpu }, () => {
      const sum = shares.reduce((a, b) => a + b, 0) || 1;
      const need = size + 0.5 * GB + Math.min(Math.max(size * 0.03, 0.12 * GB), 0.6 * GB) * (context / 4096);
      return shares.map((s, i) => (need * s) / sum + (i === mainGpu ? 0.8 * GB : 0));
    }),
  presets: () => call<PresetInfo[]>("list_presets", {}, () => [{ name: "Roleplay · Qwen3.8 27B", path: "/p/a.kcpps", modified: 0 }]),
  savePreset: (name: string, config: Kcpps) => call<string>("save_preset", { name, config }, () => name),
  deletePreset: (name: string) => call<void>("delete_preset", { name }, () => {}),
  loadConfig: (path: string) => call<Kcpps>("load_config", { path }, () => ({ model_param: "/models/Qwen3.8-27B-Q4_K_M.gguf", contextsize: 16384 })),
  /** A settings file from elsewhere: unsafe options come back in `removed`. */
  importConfig: (path: string) => call<{ config: Kcpps; removed: string[] }>("import_config", { path }),
  lastConfig: () => call<Kcpps | null>("last_config", {}, () => null),
  importSources: (path: string) => call<Source[]>("import_sources", { path }),
};

const mockCancelled = new Set<string>();

export const fmtBytes = (n: number) => (n >= 1e9 ? (n / GB).toFixed(n >= 10 * GB ? 0 : 1) + " GB" : Math.round(n / 1024 ** 2) + " MB");
