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
}
export interface Ranked {
  id: string; name: string; author: string; params: string | null; official: boolean; uncensored: boolean;
  likes: number; downloads: number; ageDays: number; score: number; source: number; baseModel: string | null; tags: string[];
}
export interface PopularItem extends Ranked { pick: QuantPick; urls: string[]; mmproj: [string, number] | null }
export interface PopularResult { items: PopularItem[]; tooBig: number; dupes: number; errors: string[] }
export interface QuantFile { quant: string; label: string; files: string[]; size: number }
export interface RepoFiles { quants: QuantFile[]; mmproj: [string, number][] }
export interface DlProgress { id: string; done: number; total: number | null; speed: number; file: string }
export interface Source {
  id: string; name: string; builtin: boolean; on: boolean; desc: string;
  comps: { official: boolean; finetune: boolean; unc: boolean } | null;
}
export interface Machine { vram: number; ram: number; unified: boolean; context: number }

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
    { id: "labs", name: "Labs' own uploads", builtin: true, desc: "Repo author is the base model author", on: true, comps: { official: true, finetune: false, unc: false } },
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
  saveSettings: (settings: Settings) => call<Settings>("save_settings", { settings }, () => (mockSettings = settings)),
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
  popular: (options: { officialOnly: boolean; showUncensored: boolean }, machine: Machine) =>
    call<PopularResult>("popular_models", { options, machine }, () => import("./mock").then((m) => m.mockPopular(options, machine, mockSettings.sources))),
  search: (query: string) => call<Ranked[]>("search_models", { query }, () => import("./mock").then((m) => m.mockSearch(query))),
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
  importSources: (path: string) => call<Source[]>("import_sources", { path }),
};

const mockCancelled = new Set<string>();

export const fmtBytes = (n: number) => (n >= 1e9 ? (n / GB).toFixed(n >= 10 * GB ? 0 : 1) + " GB" : Math.round(n / 1024 ** 2) + " MB");
