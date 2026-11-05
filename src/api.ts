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

export interface Gpu { index: number; name: string; api: "cuda" | "rocm" | "vulkan" | "metal"; vram: number; vramFree: number | null; discrete: boolean }
export interface HardwareInfo {
  os: string; arch: string; cpuName: string; logicalCpus: number; defaultThreads: number; avx2: boolean | null;
  ramTotal: number; ramAvailable: number; unifiedMemory: boolean; gpus: Gpu[]; suggestedBackend: Backend; tier: "low" | "mid" | "high";
}
export interface InstalledBinary { path: string; version: string | null; asset: string | null }
export interface BinaryStatus { installed: InstalledBinary | null; present: boolean; asset: string | null }
export interface ReleaseInfo { tag: string; publishedAt: string | null; url: string | null; notes: string; asset: string | null; size: number | null }
export interface UpdateInfo { current: string | null; latest: ReleaseInfo | null; updateAvailable: boolean }
export interface Progress { done: number; total: number | null; speed: number }
export interface Settings {
  binary: InstalledBinary | null; checkUpdatesOnStart: boolean; includePrereleases: boolean;
  modelsDir: string | null; libraryFolders: string[]; sources: Source[];
}
export interface Paths { data: string; models: string; presets: string }
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
export interface SearchHit extends Ranked { typical: [string, number] | null }
export interface SettingsPatch { checkUpdatesOnStart?: boolean; includePrereleases?: boolean; sources?: Source[] }
export interface PopularItem extends Ranked { pick: QuantPick; urls: string[]; mmproj: [string, number] | null; downloaded: boolean }
export type SortOrder = "trending" | "downloads" | "likes" | "newest";
export interface PopularResult { items: PopularItem[]; tooBig: number; dupes: number; errors: string[] }
export interface QuantFile { quant: string; label: string; files: string[]; size: number }
export interface RepoFiles { quants: QuantFile[]; mmproj: [string, number][] }
export interface DlProgress { id: string; done: number; total: number | null; speed: number; file: string }
export interface Source {
  id: string; name: string; builtin: boolean; on: boolean; desc: string;
  comps: { official: boolean; finetune: boolean; unc: boolean } | null;
}
export interface Machine { vram: number; ram: number; unified: boolean; context: number }
export type ModelKind = "Text" | "Vision" | "Image" | "Speech" | "Embedding" | "Projector";
export interface LocalModel {
  path: string; file: string; name: string; kind: ModelKind; architecture: string | null; quant: string | null; sizeLabel: string | null;
  size: number; parts: number; layers: number | null; contextLength: number | null; headCountKv: number | null; kvLen: number | null;
  mmproj: string | null; drive: string | null; modified: number | null; lastUsed: number | null;
}
export interface Drive { mount: string; name: string; total: number; free: number; removable: boolean }
export interface LibraryView { models: LocalModel[]; drives: Drive[]; folders: string[]; modelsDir: string }
export interface PresetInfo { name: string; path: string; modified: number | null }

/* ---------- plumbing ---------- */

async function call<T>(cmd: string, args: Record<string, unknown> = {}, mock?: () => T | Promise<T>): Promise<T> {
  if (!inTauri) {
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
    { index: 0, name: "NVIDIA GeForce RTX 4090", api: "cuda", vram: 24 * GB, vramFree: 23 * GB, discrete: true },
    { index: 1, name: "NVIDIA GeForce RTX 3060", api: "cuda", vram: 12 * GB, vramFree: 11.5 * GB, discrete: true },
  ],
  suggestedBackend: { kind: "cuda", gpu: null, lowvram: false, mmq: true }, tier: "high",
};

/* ---------- commands ---------- */

export const api = {
  hardware: (refresh = false) => call<HardwareInfo>("hardware_info", { refresh }, () => mockHardware),
  settings: () => call<Settings>("get_settings", {}, () => mockSettings),
  saveSettings: (patch: SettingsPatch) => call<Settings>("save_settings", { patch }, () => (mockSettings = { ...mockSettings, ...patch })),
  paths: () => call<Paths>("app_paths", {}, () => ({ data: "~/KoboldCpp Launcher", models: "~/KoboldCpp Launcher/models", presets: "~/KoboldCpp Launcher/presets" })),
  commandLine: (config: Kcpps) => call<string>("command_line", { config }),

  binaryStatus: () => call<BinaryStatus>("binary_status", {}, () => ({ installed: mockSettings.binary, present: true, asset: "koboldcpp.exe" })),
  checkUpdate: () =>
    call<UpdateInfo>("check_update", {}, () => ({
      current: mockSettings.binary?.version ?? null,
      latest: { tag: "v1.124.2", publishedAt: "2026-10-31T12:00:00Z", url: "https://github.com/LostRuins/koboldcpp/releases/tag/v1.124.2", notes: "Release notes from GitHub appear here (sample text in the browser preview).", asset: "koboldcpp.exe", size: 622841516 },
      updateAvailable: mockSettings.binary?.version !== "v1.124.2",
    })),
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
  search: (query: string) => call<SearchHit[]>("search_models", { query }, () => import("./mock").then((m) => m.mockSearch(query))),
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

  library: () => call<LibraryView>("library_scan", {}, () => import("./mock").then((m) => m.mockLibrary())),
  addFolder: (path: string) => call<Settings>("add_library_folder", { path }),
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
  lastConfig: () => call<Kcpps | null>("last_config", {}, () => null),
  importSources: (path: string) => call<Source[]>("import_sources", { path }),
};

const mockCancelled = new Set<string>();

export const fmtBytes = (n: number) => (n >= 1e9 ? (n / GB).toFixed(n >= 10 * GB ? 0 : 1) + " GB" : Math.round(n / 1024 ** 2) + " MB");
