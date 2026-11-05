// "Get and start": download what a setup needs (with progress), then launch KoboldCpp.

import { api, fmtBytes, on, type DlProgress, type Kcpps } from "./api";
import { h, toast } from "./dom";
import { launch, stop } from "./server";
import { S, render } from "./state";
import { invalidateLibrary } from "./screens/library";

export interface DlState { done: number; total: number | null; speed: number; file: string }

export function initDownloads() {
  on<DlProgress>("download-progress", (p) => {
    const d = S.dl[p.id];
    if (!d) return;
    Object.assign(d, p);
    const el = document.querySelector<HTMLElement>(`[data-dl="${CSS.escape(p.id)}"]`);
    if (!el) return render();
    el.querySelector<HTMLElement>(".bar-fill")!.style.width = pct(d) + "%";
    el.querySelector("small")!.textContent = dlText(d);
  });
}

const pct = (d: DlState) => (d.total ? Math.min(100, (d.done / d.total) * 100) : 0);
const dlText = (d: DlState) =>
  `Downloading ${fmtBytes(d.done)}${d.total ? " of " + fmtBytes(d.total) : ""}${d.speed ? ` · ${fmtBytes(d.speed)}/s` : ""} · starts automatically`;

/** Download `files`, replace their URLs in `config` with local paths, start KoboldCpp. */
export async function getAndStart(key: string, label: string, files: { url: string; size: number | null }[], config: Kcpps) {
  if (S.dl[key]) return;
  S.dl[key] = { done: 0, total: files.reduce<number | null>((a, f) => (a == null || f.size == null ? null : a + f.size), 0), speed: 0, file: "" };
  render();
  try {
    const map = new Map(await api.downloadFiles(key, files));
    const cfg: Kcpps = {};
    for (const [k, v] of Object.entries(config)) {
      cfg[k] = typeof v === "string" && map.has(v) ? map.get(v)! : Array.isArray(v) ? v.map((x) => (typeof x === "string" && map.has(x) ? map.get(x)! : x)) : v;
    }
    delete S.dl[key];
    invalidateLibrary();
    if (S.running) await stop();
    await launch(label, cfg);
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

/** Button or progress bar for one setup. */
export function goControl(key: string, label: string, onGo: () => void, opts: { installed?: boolean; text?: string } = {}) {
  const d = S.dl[key];
  if (d)
    return h("div", { class: "prog", "data-dl": key },
      h("div", { class: "go" },
        h("div", { class: "bar-track", style: "flex:1" }, h("div", { class: "bar-fill", style: `width:${pct(d)}%` })),
        h("button", { onclick: () => cancel(key) }, "Cancel")),
      h("small", {}, dlText(d)));
  if (S.running === label && S.server.state !== "stopped") return h("button", { onclick: stop }, "Stop");
  return h("button", { class: opts.installed ? "" : "primary", onclick: onGo }, opts.text ?? (opts.installed ? "Start" : "Get and start"));
}
