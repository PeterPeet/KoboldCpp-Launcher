// Advanced setup: every control edits the launch config (`S.cfg`), which is what KoboldCpp gets.
// The command line below is generated from the same config by the Rust side.

import { api, fmtBytes, inTauri, type Gpu, type Kcpps, type LocalModel, type PresetInfo } from "../api";
import { $, h, select, toggle, toast, type Kid } from "../dom";
import { isActive, launch, stop } from "../server";
import { reveal } from "../dialogs/update";
import { getTheme, setTheme, type Theme } from "../theme";
import { S, render } from "../state";

const ICONS: Record<string, string> = {
  model: "M12 3l8 4.5v9L12 21l-8-4.5v-9z M12 12l8-4.5 M12 12v9 M12 12L4 7.5",
  hardware: "M7 7h10v10H7z M9 3v4 M15 3v4 M9 17v4 M15 17v4 M3 9h4 M3 15h4 M17 9h4 M17 15h4",
  context: "M4 6h16 M4 12h16 M4 18h10",
  images: "M4 5h16v14H4z M4 16l5-5 4 4 3-3 4 4",
  voice: "M12 3a3 3 0 013 3v6a3 3 0 01-6 0V6a3 3 0 013-3z M5 11a7 7 0 0014 0 M12 18v3",
  network: "M12 3v6 M5 15v-3h14v3 M3 15h4v6H3z M10 15h4v6h-4z M17 15h4v6h-4z",
  presets: "M6 3h12v18l-6-4-6 4z",
  logs: "M5 4h14v16H5z M8 8h8 M8 12h8 M8 16h5",
  appearance: "M12 3a9 9 0 100 18c1 0 1.5-.8 1.5-1.5 0-.9-.6-1.3-.6-2 0-.8.6-1.5 1.5-1.5H17a4 4 0 004-4c0-5-4-9-9-9z M7.5 11.5h.01 M10 7.5h.01 M14 7.5h.01 M16.5 11h.01",
};
const PAGES: [string, string][] = [
  ["model", "Model"], ["hardware", "Hardware"], ["context", "Context and memory"], ["images", "Images"],
  ["voice", "Voice and audio"], ["network", "Network"], ["presets", "Presets"], ["logs", "Logs"], ["appearance", "Appearance"],
];

/* ---------- config state ---------- */

let loaded = false;
let models: LocalModel[] = [];
let presets: PresetInfo[] | null = null;
let cli = "";
let cliTimer: number | undefined;
let usage: number[] = [];
let presetName = "";

async function init() {
  loaded = true;
  try {
    const last = await api.lastConfig();
    if (last && Object.keys(S.cfg).length === 0) S.cfg = last;
  } catch { /* first run */ }
  try {
    models = (await api.library()).models;
  } catch { /* no library yet */ }
  refreshCli();
  render();
}

const get = (k: string) => S.cfg[k];
const num = (k: string, d: number) => (typeof get(k) === "number" ? (get(k) as number) : d);
const str = (k: string) => (typeof get(k) === "string" ? (get(k) as string) : "");

function set(patch: Kcpps) {
  Object.assign(S.cfg, patch);
  refreshCli();
  render();
}

function refreshCli() {
  clearTimeout(cliTimer);
  cliTimer = window.setTimeout(async () => {
    try {
      cli = inTauri ? await api.commandLine(S.cfg) : "koboldcpp " + Object.entries(S.cfg).filter(([, v]) => v != null && v !== false && v !== "").map(([k, v]) => `--${k} ${v === true ? "" : Array.isArray(v) ? v.join(" ") : v}`.trim()).join(" ");
    } catch (e) {
      cli = String(e);
    }
    const c = $("#cliText");
    if (c) c.textContent = cli;
    refreshUsage();
  }, 120);
}

/* ---------- hardware helpers ---------- */

type BackendKind = "auto" | "cuda" | "vulkan" | "cpu";
function backend(): BackendKind {
  if (Array.isArray(get("usecuda"))) return "cuda";
  if (Array.isArray(get("usevulkan"))) return "vulkan";
  if (get("usecpu") === true) return "cpu";
  return "auto";
}
const gpus = (): Gpu[] => S.hw?.gpus ?? [];
/** GPU indices the config uses (all when none is named). */
function chosenGpus(): number[] {
  const all = gpus().map((g) => g.index);
  const b = backend();
  if (b === "cuda") {
    const ids = (get("usecuda") as unknown[]).map(Number).filter((n) => !Number.isNaN(n));
    const ts = get("tensor_split");
    if (ids.length === 1 && !Array.isArray(ts)) return ids;
  }
  if (b === "vulkan") {
    const ids = (get("usevulkan") as number[]) ?? [];
    return ids.length ? ids : all;
  }
  if (b === "cpu") return [];
  const ts = get("tensor_split");
  if (Array.isArray(ts) && ts.length === all.length) return all.filter((_, i) => (ts[i] as number) > 0);
  return all;
}

function setBackend(b: BackendKind, ids = chosenGpus()) {
  const many = ids.length > 1;
  const patch: Kcpps = { usecuda: null, usevulkan: null, usecpu: false };
  if (b === "cuda") patch.usecuda = many || ids.length === 0 ? ["normal"] : ["normal", String(ids[0])];
  if (b === "vulkan") patch.usevulkan = ids.length === gpus().length ? [] : ids;
  if (b === "cpu") patch.usecpu = true;
  if (!many) patch.tensor_split = null;
  else if (!Array.isArray(get("tensor_split"))) patch.tensor_split = ids.map(() => Math.round(100 / ids.length));
  set(patch);
}

function backendOptions(): [string, string][] {
  const hw = S.hw;
  if (hw?.unifiedMemory) return [["auto", "Metal (automatic)"], ["cpu", "CPU only"]];
  const o: [string, string][] = [["auto", "Automatic"]];
  if (gpus().some((g) => g.api === "cuda")) o.push(["cuda", "CUDA (NVIDIA)"]);
  if (gpus().some((g) => g.api === "rocm")) o.push(["cuda", "ROCm (AMD)"]);
  o.push(["vulkan", "Vulkan (any GPU)"], ["cpu", "CPU only"]);
  return o;
}

const textModel = () => models.find((m) => m.path === str("model_param"));

async function refreshUsage() {
  const ids = chosenGpus();
  const m = textModel();
  if (ids.length < 2 || !m || backend() === "cpu") { usage = []; return; }
  const ts = Array.isArray(get("tensor_split")) ? (get("tensor_split") as number[]) : ids.map(() => 50);
  usage = await api.splitUsage(m.size, num("contextsize", 16384), ts, Math.max(0, num("maingpu", 0)));
  document.querySelectorAll<HTMLElement>("[data-gpu-bar]").forEach((el) => {
    const i = Number(el.dataset.gpuBar);
    const cap = gpus()[i]?.vram ?? 1;
    const u = usage[i] ?? 0;
    el.querySelector<HTMLElement>(".bar-fill")!.style.width = `${Math.min(100, (u / cap) * 100)}%`;
    el.querySelector<HTMLElement>(".bar-fill")!.style.background = u > cap ? "var(--bad)" : i ? "#1d9e75" : "var(--accent)";
    const s = el.querySelector("small")!;
    s.textContent = `${fmtBytes(u)} used${u > cap ? " · too full, move the divider" : ""}${num("maingpu", 0) === i || (num("maingpu", -1) < 0 && i === 0) ? " · main GPU" : ""}`;
    s.setAttribute("style", u > cap ? "color:var(--bad)" : "");
  });
}

/* ---------- view helpers ---------- */

function icon(k: string) {
  const ns = "http://www.w3.org/2000/svg";
  const s = document.createElementNS(ns, "svg");
  for (const [a, v] of Object.entries({ viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", "stroke-width": "1.8", "stroke-linecap": "round", "stroke-linejoin": "round", "aria-hidden": "true" }))
    s.setAttribute(a, v);
  const p = document.createElementNS(ns, "path");
  p.setAttribute("d", ICONS[k]);
  s.append(p);
  return s;
}
/** Settings row. The control gets the row's label as its accessible name. */
function row(label: Kid, ctrl: Kid, hint?: string) {
  if (typeof label === "string" && ctrl instanceof HTMLElement) {
    const field = ctrl.matches("select,input") ? ctrl : ctrl.querySelector("select,input");
    field?.setAttribute("aria-label", label);
  }
  return h("div", { class: "r" }, h("div", {}, label, hint ? h("small", {}, hint) : null), ctrl);
}
const title = (t: string) => h("h1", { style: "font-size:22px" }, t);

/** File chooser from the library plus "Other file…" and "None". */
function modelSelect(id: string, key: string, kinds: string[], opts: { none?: string; match?: (m: LocalModel) => boolean } = {}) {
  const cur = str(key);
  const list = models.filter((m) => kinds.includes(m.kind) && (!opts.match || opts.match(m)));
  const options: [string, string][] = [];
  if (opts.none) options.push(["", opts.none]);
  for (const m of list) options.push([m.path, `${m.name}${m.quant ? " · " + m.quant : ""} · ${fmtBytes(m.size)}`]);
  if (cur && !list.some((m) => m.path === cur)) options.push([cur, cur.startsWith("http") ? `Download: ${cur.split("/").pop()}` : cur.split(/[\\/]/).pop()!]);
  options.push(["__pick", "Other file…"]);
  return select(id, cur, options, async (v) => {
    if (v !== "__pick") {
      const patch: Kcpps = { [key]: v };
      const m = models.find((x) => x.path === v);
      if (key === "model_param" && m?.mmproj && !str("mmproj")) patch.mmproj = m.mmproj;
      return set(patch);
    }
    if (!inTauri) return toast("Files can be picked in the desktop app");
    const { open } = await import("@tauri-apps/plugin-dialog");
    const p = await open({ multiple: false, filters: [{ name: "Models", extensions: ["gguf", "safetensors", "bin"] }] });
    if (typeof p === "string") set({ [key]: p });
    else render();
  });
}

/* ---------- pages ---------- */

function hardwarePage(pane: HTMLElement) {
  const b = backend();
  const ids = chosenGpus();
  pane.append(title("Hardware"));
  const grp = h("div", { class: "grp" },
    row("Backend", select("backend", b, backendOptions(), (v) => setBackend(v as BackendKind)),
      S.hw?.unifiedMemory ? "Apple Silicon always uses Metal; layers stay on the GPU." : undefined));
  if (b !== "cpu" && !S.hw?.unifiedMemory)
    for (const g of gpus())
      grp.append(row(`Use ${g.name} · ${fmtBytes(g.vram)}`, toggle("gpu" + g.index, ids.includes(g.index), (v) => {
        const next = v ? [...ids, g.index].sort() : ids.filter((i) => i !== g.index);
        if (!next.length) { toast("Keep at least one GPU, or choose CPU only"); return render(); }
        setBackend(b === "auto" ? (g.api === "vulkan" ? "vulkan" : "cuda") : b, next);
      })));
  const layers = num("gpulayers", -1);
  const layerInput = h("input", { type: "number", id: "layers", min: "0", max: "999", value: layers < 0 ? "" : layers, placeholder: "auto", style: "width:90px", "aria-label": "Layers on GPU" }) as HTMLInputElement;
  layerInput.addEventListener("change", () => set({ gpulayers: layerInput.value === "" ? -1 : Math.max(0, +layerInput.value) }));
  const m = textModel();
  grp.append(row("Layers on GPU", h("span", { style: "display:inline-flex;gap:8px;align-items:center" }, layerInput, m?.layers ? h("span", { class: "mono", style: "font-size:12px;color:var(--fg-3)" }, `of ${m.layers + 1}`) : null),
    "Empty = automatic. KoboldCpp fits as many as the free memory allows."));
  pane.append(grp);

  if (ids.length > 1 && b !== "cpu") {
    const ts = (Array.isArray(get("tensor_split")) ? (get("tensor_split") as number[]) : ids.map(() => 50)).slice(0, 2);
    const share = Math.round((ts[0] / (ts[0] + ts[1] || 1)) * 100);
    const L = m?.layers ? m.layers + 1 : null;
    const range = h("input", { type: "range", id: "split", min: "10", max: "90", step: "5", value: share, "aria-label": "Share of the model on GPU 0" }) as HTMLInputElement;
    range.addEventListener("input", () => { set({ tensor_split: [+range.value, 100 - +range.value] }); $("#split")?.focus(); });
    const bar = (i: number) => {
      const g = gpus()[i];
      return h("div", { "data-gpu-bar": i }, `${g.name} · ${fmtBytes(g.vram)}`,
        h("div", { class: "bar-track", style: "margin:5px 0" }, h("div", { class: "bar-fill", style: "width:0%" })),
        h("small", {}, m ? "Calculating…" : "Pick a model to see memory use"));
    };
    pane.append(
      h("p", { class: "glabel" }, "Split between GPUs"),
      h("div", { class: "grp", style: "padding:14px;display:grid;gap:12px" },
        h("div", { class: "split" },
          h("div", { class: "g0", style: `width:${share}%` }, `GPU 0${L ? ` · ${Math.round((L * share) / 100)} layers` : ` · ${share}%`}`),
          h("div", { class: "g1", style: `width:${100 - share}%` }, `GPU 1${L ? ` · ${L - Math.round((L * share) / 100)} layers` : ` · ${100 - share}%`}`)),
        range,
        h("div", { class: "gpus" }, bar(0), bar(1))),
      h("div", { class: "grp" },
        row("Split mode", select("mode", str("splitmode") || "layer", [["layer", "By layer"], ["row", "By row"]], (v) => set({ splitmode: v })), "By row can be faster with two similar GPUs"),
        row("Main GPU", select("main", Math.max(0, num("maingpu", 0)), gpus().map((g) => [g.index, `GPU ${g.index} · ${g.name}`]), (v) => set({ maingpu: +v })))));
  }
  const threads = h("input", { type: "number", id: "threads", min: "1", max: "256", value: num("threads", 0) > 0 ? num("threads", 0) : "", placeholder: String(S.hw?.defaultThreads ?? "auto"), style: "width:90px", "aria-label": "CPU threads" }) as HTMLInputElement;
  threads.addEventListener("change", () => set({ threads: threads.value === "" ? null : +threads.value }));
  pane.append(h("div", { class: "grp" }, row("CPU threads", threads, `Empty = KoboldCpp's choice (${S.hw?.defaultThreads ?? "auto"} on this PC)`)));
}

function contextPage(pane: HTMLElement) {
  const ctxOpts: [number, string][] = [2048, 4096, 8192, 12288, 16384, 24576, 32768, 49152, 65536, 98304, 131072].map((n) => [n, n.toLocaleString()]);
  const cur = num("contextsize", 16384);
  if (!ctxOpts.some(([n]) => n === cur)) ctxOpts.push([cur, cur.toLocaleString()]);
  const m = textModel();
  pane.append(title("Context and memory"),
    h("div", { class: "grp" },
      row("Context size", select("ctx", cur, ctxOpts.sort((a, b) => a[0] - b[0]), (v) => set({ contextsize: +v })),
        `How much of the chat the model can remember${m?.contextLength ? ` (this model supports up to ${Math.round(m.contextLength / 1024)}K)` : ""}`),
      row("Flash attention", toggle("fa", get("noflashattention") !== true, (v) => set({ noflashattention: !v })), "Less memory, usually faster"),
      row("Cache precision", select("kv", str("quantkv") || "f16", [["f16", "Full (f16)"], ["q8_0", "Half (8-bit)"], ["q4_0", "Quarter (4-bit)"]], (v) => set({ quantkv: v })), "Lower saves memory, costs a little quality"),
      row("Batch size", select("batch", num("batchsize", 512), [[128, "128"], [256, "256"], [512, "512 (default)"], [1024, "1024"], [2048, "2048"]], (v) => set({ batchsize: +v })), "Bigger reads long prompts faster, needs more memory")));
}

function modelPage(pane: HTMLElement) {
  pane.append(title("Model"),
    h("div", { class: "grp" },
      row("Text model", modelSelect("textModel", "model_param", ["Text", "Vision"], { none: "None (only image, voice or search models)" })),
      row("Vision file", modelSelect("mmproj", "mmproj", ["Projector"], { none: "None" }), "Lets the model see images (mmproj)"),
      row("Draft model", modelSelect("draft", "draftmodel", ["Text"], { none: "None" }), "A small model of the same family speeds up answers"),
      row("Search and memory", modelSelect("embd", "embeddingsmodel", ["Embedding"], { none: "None" }), "Embeddings model for documents and memory"),
      row("Use MTP layers", toggle("usemtp", get("usemtp") === true, (v) => set({ usemtp: v })),
        textModel()?.mtp ? "This model has MTP layers: faster answers, needs a bit more memory" : "Only for models with MTP layers (e.g. some Qwen 3.5+ quants)"),
      row("Use the model's chat template", toggle("jinja", get("jinja") === true, (v) => set({ jinja: v })), "Jinja; needed for tool use with some models")));
}

function imagesPage(pane: HTMLElement) {
  pane.append(title("Images"),
    h("div", { class: "grp" },
      row("Image model", modelSelect("sdmodel", "sdmodel", ["Image"], { none: "None" })),
      row("Smaller image model in memory", toggle("sdquant", num("sdquant", 0) > 0, (v) => set({ sdquant: v ? 2 : 0 })), "Quantizes it on load: less memory, slightly lower quality")));
}

function voicePage(pane: HTMLElement) {
  pane.append(title("Voice and audio"),
    h("div", { class: "grp" },
      row("Voice input", modelSelect("whisper", "whispermodel", ["Speech"], { none: "None", match: (m) => /whisper/i.test(m.file) }), "Speech to text (Whisper)"),
      row("Voice output", modelSelect("tts", "ttsmodel", ["Speech"], { none: "None", match: (m) => !/whisper/i.test(m.file) }), "Text to speech (e.g. Kokoro)")));
}

function networkPage(pane: HTMLElement) {
  const port = h("input", { type: "number", id: "port", min: "1024", max: "65535", value: num("port", 5001), style: "width:100px", "aria-label": "Port" }) as HTMLInputElement;
  port.addEventListener("change", () => { const p = Math.min(65535, Math.max(1024, +port.value || 5001)); set({ port: p, port_param: p }); });
  pane.append(title("Network"),
    h("div", { class: "grp" },
      row("Port", port),
      row("Allow other devices on this network", toggle("lan", str("host") !== "127.0.0.1", (v) => set({ host: v ? "" : "127.0.0.1" })), "Off: only this computer can connect"),
      row("Remote access tunnel", toggle("tunnel", get("remotetunnel") === true, (v) => set({ remotetunnel: v })), "Public link through Cloudflare. Anyone with the link can use it."),
      row("Open the browser when ready", toggle("launch", get("launch") === true, (v) => set({ launch: v })))));
}

async function loadPresets() {
  try { presets = await api.presets(); } catch { presets = []; }
  render();
}

async function importConfig() {
  if (!inTauri) return toast("Import works in the desktop app");
  const { open } = await import("@tauri-apps/plugin-dialog");
  const p = await open({ multiple: false, filters: [{ name: "KoboldCpp settings", extensions: ["kcpps", "kcppt"] }] });
  if (typeof p !== "string") return;
  try {
    S.cfg = await api.loadConfig(p);
    refreshCli();
    toast("Settings loaded");
    render();
  } catch (e) {
    toast(String(e));
  }
}

function presetsPage(pane: HTMLElement) {
  if (presets === null) loadPresets();
  pane.append(title("Presets"));
  const grp = h("div", { class: "grp" });
  for (const p of presets ?? [])
    grp.append(row(p.name, h("span", { style: "display:inline-flex;gap:6px" },
      h("button", { onclick: async () => { S.cfg = await api.loadConfig(p.path); refreshCli(); toast(`${p.name} loaded`); render(); } }, "Load"),
      h("button", { class: "ghost", onclick: async () => { await api.deletePreset(p.name); toast(`${p.name} deleted`); loadPresets(); } }, "Delete"))));
  if (!presets?.length) grp.append(h("div", { class: "empty" }, presets ? "No presets yet. Use “Save as preset” below." : "Loading…"));
  pane.append(grp, h("div", { class: "btns", style: "justify-content:flex-start" },
    h("button", { onclick: importConfig }, "Import .kcpps or .kcppt…")));
}

/* ---------- logs ---------- */

let logs: { dir: string; koboldcpp: string; launcher: string } | null = null;
let logTab: "koboldcpp" | "launcher" = "koboldcpp";
let logTimer: number | undefined;

async function loadLogs() {
  try {
    logs = await api.readLogs();
  } catch (e) {
    logs = { dir: "", koboldcpp: String(e), launcher: "" };
  }
  const pre = $("#logText");
  if (pre && logs) {
    const atEnd = pre.scrollTop + pre.clientHeight >= pre.scrollHeight - 20;
    pre.textContent = logs[logTab] || "Nothing logged yet.";
    if (atEnd) pre.scrollTop = pre.scrollHeight;
  } else {
    render();
  }
}

function logsPage(pane: HTMLElement) {
  if (!logs) loadLogs();
  // Refresh while this page is open.
  clearInterval(logTimer);
  logTimer = window.setInterval(() => {
    if (S.adv.page !== "logs" || location.hash !== "#advanced") return clearInterval(logTimer);
    loadLogs();
  }, 2000);
  const tabs = h("div", { class: "tabs", role: "tablist" },
    ...([["koboldcpp", "KoboldCpp"], ["launcher", "Launcher"]] as const).map(([k, l]) =>
      h("button", { role: "tab", "aria-selected": String(logTab === k), onclick: () => { logTab = k; render(); } }, l)));
  const pre = h("pre", { class: "log big", id: "logText" }, logs ? logs[logTab] || "Nothing logged yet." : "Loading…");
  setTimeout(() => (pre.scrollTop = pre.scrollHeight));
  pane.append(title("Logs"),
    h("p", { class: "note", style: "margin:0" }, logTab === "koboldcpp"
      ? "Everything KoboldCpp printed during the last start (koboldcpp.log)."
      : "What the launcher did: starts, stops, downloads, updates and errors (launcher.log)."),
    h("div", { style: "display:flex;justify-content:space-between;align-items:flex-end;gap:10px;flex-wrap:wrap" },
      tabs,
      h("div", { style: "display:flex;gap:6px" },
        h("button", { class: "ghost", onclick: loadLogs }, "Refresh"),
        h("button", { onclick: async () => reveal(`${(logs?.dir || (await api.paths()).logs)}/${logTab}.log`) }, "Open folder"))),
    pre);
}

/* ---------- appearance ---------- */

function appearancePage(pane: HTMLElement) {
  const cur = getTheme();
  // Small picture of each scheme: top bar + window, in its own colors.
  const choice = (id: Theme, label: string, hint: string, top: string, body: string, accent: string) =>
    h("button", { class: "theme", role: "radio", "aria-checked": String(cur === id), onclick: () => { setTheme(id); render(); } },
      h("div", { class: "sw" },
        h("i", { style: `background:${top}` }),
        h("i", { style: `background:${body};position:relative` },
          h("i", { style: `position:absolute;left:12px;bottom:12px;width:64px;height:16px;border-radius:6px;background:${accent}` }))),
      h("div", { class: "lbl" }, label, h("small", {}, hint)));
  pane.append(title("Appearance"),
    h("p", { class: "note", style: "margin:0" }, "Color scheme of the launcher. Applies right away."),
    h("div", { class: "themes", role: "radiogroup", "aria-label": "Color scheme" },
      choice("light", "Light", "Bright and neutral", "#ffffff", "#eef0f4", "#3451d1"),
      choice("lite", "KoboldAI Lite", "Lite's default color scheme (Modern)", "#32496d", "#0b141a", "#415577")));
}

/* ---------- screen ---------- */

export function renderAdvanced() {
  if (!loaded) init();
  const A = S.adv;
  const root = h("div", { class: "adv" });
  const nav = h("nav", { class: "snav", "aria-label": "Settings" },
    ...PAGES.map(([k, l]) => h("button", { "aria-current": String(A.page === k), onclick: () => { A.page = k; render(); } }, icon(k), l)));
  root.append(h("aside", { class: "side" }, nav));
  const pane = h("section", { class: "pane" });
  root.append(pane);

  ({ hardware: hardwarePage, context: contextPage, model: modelPage, images: imagesPage, voice: voicePage, network: networkPage, presets: presetsPage, logs: logsPage, appearance: appearancePage } as Record<string, (p: HTMLElement) => void>)[A.page]?.(pane);
  // The log page is a viewer: no command line or start buttons below it.
  if (A.page === "logs" || A.page === "appearance") return root;

  const copy = async () => {
    try { await navigator.clipboard.writeText(cli); toast("Command copied"); }
    catch {
      const r = document.createRange();
      r.selectNodeContents($("#cliText")!);
      getSelection()?.removeAllRanges();
      getSelection()?.addRange(r);
      toast("Selected, press Cmd+C to copy");
    }
  };
  pane.append(h("div", { style: "display:grid;gap:6px" },
    h("p", { class: "glabel", style: "margin:0 0 0 4px" }, "Same setup as a command"),
    h("div", { class: "cli" }, h("code", { id: "cliText" }, cli || "…"), h("button", { onclick: copy }, "Copy"))));

  const nameInput = h("input", { id: "presetName", placeholder: "Preset name", value: presetName, "aria-label": "Preset name", style: "border:1px solid var(--line-2);border-radius:8px;padding:5px 10px;background:var(--win);font-size:13px;min-width:160px" }) as HTMLInputElement;
  nameInput.addEventListener("input", () => (presetName = nameInput.value));
  const save = async () => {
    if (!presetName.trim()) { nameInput.focus(); return toast("Give the preset a name"); }
    try { await api.savePreset(presetName.trim(), S.cfg); toast(`Saved as “${presetName.trim()}”`); presetName = ""; presets = null; render(); }
    catch (e) { toast(String(e)); }
  };
  const label = textModel()?.name ?? (str("model_param").split(/[\\/]/).pop() || "KoboldCpp");
  pane.append(h("div", { class: "btns" },
    nameInput, h("button", { onclick: save }, "Save as preset"),
    h("button", { class: "primary", onclick: async () => { if (isActive()) await stop(); launch(label, { ...S.cfg }); } }, isActive() ? "Restart server" : "Start server")));
  if (A.page === "hardware") setTimeout(refreshUsage);
  return root;
}

