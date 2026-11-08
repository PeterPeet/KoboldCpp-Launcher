// Downloads: "Get and start" (download, then launch KoboldCpp), "Get" (download only), and the
// download indicator in the top bar (a progress ring; click for the list of running downloads).

import { api, fmtBytes, on, type DlProgress, type Kcpps } from "./api";
import { $, h, toast } from "./dom";
import { launch, stop } from "./server";
import { S, render } from "./state";
import { invalidateLibrary } from "./screens/library";

export interface DlState {
  done: number; total: number | null; speed: number; file: string;
  /** What is downloading (template or model name). */
  label: string;
  /** Start KoboldCpp when done ("Get and start"); false for "Get". */
  start: boolean;
}

export function initDownloads() {
  on<DlProgress>("download-progress", (p) => {
    const d = S.dl[p.id];
    if (!d) return;
    Object.assign(d, { done: p.done, total: p.total, speed: p.speed, file: p.file });
    // Update progress bars in place. A row not on screen draws its state from S.dl when it
    // appears; redrawing the screen here would rebuild inputs while the user types.
    document.querySelectorAll<HTMLElement>(`[data-dl="${CSS.escape(p.id)}"]`).forEach((el) => {
      el.querySelector<HTMLElement>(".bar-fill")!.style.width = pct(d) + "%";
      el.querySelector("small")!.textContent = dlText(d);
    });
    drawIndicator();
  });
  $("#dlBtn")?.addEventListener("click", (e) => {
    e.stopPropagation();
    menuOpen = !menuOpen;
    drawIndicator();
  });
  document.addEventListener("click", (e) => {
    if (menuOpen && !(e.target as Element).closest("#dlMenu")) {
      menuOpen = false;
      drawIndicator();
    }
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && menuOpen) {
      menuOpen = false;
      drawIndicator();
    }
  });
}

const pct = (d: { done: number; total: number | null }) => (d.total ? Math.min(100, (d.done / d.total) * 100) : 0);
const dlText = (d: DlState) =>
  `Downloading ${fmtBytes(d.done)}${d.total ? " of " + fmtBytes(d.total) : ""}${d.speed ? ` · ${fmtBytes(d.speed)}/s` : ""}${d.start ? " · starts automatically" : ""}`;

/** Download `files`. With `start`, replace their URLs in `config` with local paths and start
 *  KoboldCpp; with `preset`, also save that config as a preset (e.g. to take it to another PC). */
export async function getAndStart(key: string, label: string, files: { url: string; size: number | null }[], config: Kcpps, opts: { start?: boolean; preset?: string } = {}) {
  if (S.dl[key]) return;
  const start = opts.start ?? true;
  S.dl[key] = { done: 0, total: files.reduce<number | null>((a, f) => (a == null || f.size == null ? null : a + f.size), 0), speed: 0, file: "", label, start };
  render();
  try {
    const map = new Map(await api.downloadFiles(key, files));
    const cfg: Kcpps = {};
    for (const [k, v] of Object.entries(config)) {
      cfg[k] = typeof v === "string" && map.has(v) ? map.get(v)! : Array.isArray(v) ? v.map((x) => (typeof x === "string" && map.has(x) ? map.get(x)! : x)) : v;
    }
    delete S.dl[key];
    invalidateLibrary();
    // Downloaded flags on the start screen and in Discover are out of date now.
    import("./screens/start").then((m) => m.refreshDownloaded());
    if (opts.preset) {
      try {
        await api.savePreset(opts.preset, cfg);
      } catch (e) {
        toast(`Downloaded, but the preset could not be saved: ${e}`);
      }
    }
    if (!start) {
      render();
      toast(opts.preset ? `${label} downloaded and saved as preset “${opts.preset}”` : `${label} downloaded`);
      return;
    }
    if (S.running) await stop();
    await launch(label, cfg);
    S.runningKey = key;
    render();
  } catch (e) {
    delete S.dl[key];
    render();
    const msg = String(e);
    toast(msg.includes("cancelled") ? "Download cancelled. It continues where it stopped next time." : msg);
  }
}

export function cancel(key: string) {
  api.cancelDownload(key);
}

/** Button(s) or progress bar for one setup. `onGo(start)`: start = "Get and start", false = "Get".
 *  `getOnly`: a single download button with this text (e.g. for setups too big for this PC). */
export function goControl(key: string, label: string, onGo: (start: boolean) => void, opts: { installed?: boolean; text?: string; getOnly?: string; getTitle?: string } = {}) {
  const d = S.dl[key];
  if (d)
    return h("div", { class: "prog", "data-dl": key },
      h("div", { class: "go" },
        h("div", { class: "bar-track", style: "flex:1" }, h("div", { class: "bar-fill", style: `width:${pct(d)}%` })),
        h("button", { onclick: () => cancel(key) }, "Cancel")),
      h("small", {}, dlText(d)));
  if (S.running === label && (!S.runningKey || S.runningKey === key) && S.server.state !== "stopped") return h("button", { onclick: stop }, "Stop");
  if (opts.getOnly) return h("button", { class: "get-only", onclick: () => onGo(false), title: opts.getTitle }, opts.getOnly);
  // Already on this computer: green Start (nothing to download).
  if (opts.installed) return h("button", { class: "go-start", onclick: () => onGo(true), title: "Already downloaded" }, opts.text ?? "Start");
  // Two thirds "Get and start", one third "Get" (download only).
  return h("div", { class: "split-go" },
    h("button", { class: "primary", onclick: () => onGo(true) }, opts.text ?? "Get and start"),
    h("button", { class: "get-only", onclick: () => onGo(false), title: "Download only, start later" }, "Get"));
}

/* ---------- top bar indicator ---------- */

let menuOpen = false;

/** Ring with the overall progress of all running downloads; hidden when none run. */
export function drawIndicator() {
  const btn = $("#dlBtn");
  const menu = $("#dlMenu");
  if (!btn || !menu) return;
  const list = Object.entries(S.dl);
  btn.hidden = list.length === 0;
  if (!list.length) menuOpen = false;
  const known = list.filter(([, d]) => d.total);
  const total = known.reduce((a, [, d]) => a + (d.total ?? 0), 0);
  const done = known.reduce((a, [, d]) => a + Math.min(d.done, d.total ?? 0), 0);
  const p = total ? done / total : 0;
  const ring = btn.querySelector<SVGCircleElement>(".ring-fill");
  // Circumference of r=8: 50.27.
  if (ring) ring.style.strokeDashoffset = String(50.27 * (1 - p));
  const n = btn.querySelector(".dl-n");
  if (n) n.textContent = String(list.length);
  btn.title = `${list.length} download${list.length === 1 ? "" : "s"} running · ${Math.round(p * 100)} %`;
  btn.setAttribute("aria-expanded", String(menuOpen));
  menu.hidden = !menuOpen;
  if (!menuOpen) return;
  menu.replaceChildren(
    h("div", { class: "dl-head" }, "Downloading"),
    ...list.map(([key, d]) =>
      h("div", { class: "dl-item", "data-dl": key },
        h("div", { class: "dl-top" }, h("b", {}, d.label), h("button", { class: "ghost", onclick: (e: Event) => { e.stopPropagation(); cancel(key); } }, "Cancel")),
        d.file ? h("div", { class: "dl-file mono" }, d.file) : null,
        h("div", { class: "bar-track" }, h("div", { class: "bar-fill", style: `width:${pct(d)}%` })),
        h("small", {}, dlText(d)))));
}
