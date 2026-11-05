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
  modelsDir: string | null; libraryFolders: string[];
}
export interface Paths { data: string; models: string; presets: string }
export type ServerStatus =
  | { state: "stopped" }
  | { state: "starting"; port: number }
  | { state: "running"; port: number }
  | { state: "failed"; message: string };
export type ServerEvent = { type: "status"; status: ServerStatus } | { type: "log"; line: string };
export type Kcpps = Record<string, unknown>;

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
let mockSettings: Settings = { binary: { path: "/Applications/koboldcpp", version: "v1.123.1", asset: "koboldcpp-mac-arm64" }, checkUpdatesOnStart: true, includePrereleases: false, modelsDir: null, libraryFolders: [] };
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
};

export const fmtBytes = (n: number) => (n >= 1e9 ? (n / GB).toFixed(n >= 10 * GB ? 0 : 1) + " GB" : Math.round(n / 1024 ** 2) + " MB");
