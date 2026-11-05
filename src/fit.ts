// Fit, ranking and sample download logic (placeholder until the Rust side does real memory maths).

import { MODELS, TIERS, type Fit, type HfModel, type Template } from "./data";
import { gb, h, toast } from "./dom";
import { S, render, vram } from "./state";
import { launch, stop } from "./server";
import { sampleConfig } from "./data";

export interface Pick { q: string; size: number; fit: Fit }

/** Largest of Q6_K / Q5_K_M / Q4_K_M that fits (Q8_0 only for tiny models), else partial or CPU. */
export function pickQuant(m: HfModel): Pick | null {
  const v = vram();
  const ram = TIERS[S.tier].ram;
  const order = ["Q8_0", "Q6_K", "Q5_K_M", "Q4_K_M"];
  const cands = order.filter((q) => m.quants[q] != null && (q !== "Q8_0" || m.quants[q] < 3));
  const keys = Object.keys(m.quants);
  const qn = m.quants.Q4_K_M != null ? "Q4_K_M" : keys[keys.length - 1];
  const q4 = m.quants[qn];
  if (v === 0) return q4 <= ram * 0.6 ? { q: qn, size: q4, fit: "cpu" } : null;
  for (const q of cands) if (m.quants[q] + 1.5 <= v) return { q, size: m.quants[q], fit: "full" };
  if (q4 <= v + ram * 0.5) return { q: qn, size: q4, fit: "partial" };
  return null;
}

/** score = (log10(downloads) + 0.6·log10(likes)) × max(0.35, 0.5^(age/180)) */
export const score = (m: HfModel) =>
  (Math.log10(m.dl + 1) + 0.6 * Math.log10(m.likes + 1)) * Math.max(0.35, Math.pow(0.5, m.age / 180));

export function speed(p: Pick) {
  const base = p.fit === "full" ? 520 / (p.size + 2) : p.fit === "cpu" ? 60 / (p.size + 2) : 110 / (p.size + 2);
  return Math.max(1, Math.round(base));
}

export function fitPill(f: Fit) {
  if (f === "full") return h("span", { class: "pill p-ok" }, "Fits");
  if (f === "partial") return h("span", { class: "pill p-warn" }, "Partly on CPU");
  if (f === "cpu") return h("span", { class: "pill p-mute" }, "Runs on CPU");
  return h("span", { class: "pill p-bad" }, "Too big");
}

/** Index of the highest source that offers this model, or -1. */
function sourceFor(m: HfModel) {
  const ranked = S.sources.filter((s) => s.on && s.comps);
  for (let i = 0; i < ranked.length; i++) {
    const s = ranked[i];
    const matches = s.id === "labs" ? !!m.labs : s.name === m.up;
    if (!matches) continue;
    const c = s.comps!;
    const ok = m.unc ? c.unc : m.kind === "official" ? c.official : c.finetune;
    if (ok) return i;
  }
  return -1;
}

export interface Ranked { m: HfModel; p: Pick; si: number; s: number }

export function popular() {
  let tooBig = 0;
  const pool: Ranked[] = [];
  for (const m of MODELS) {
    if (S.official && m.kind !== "official") continue;
    if (m.unc && !S.uncensored) continue;
    const si = sourceFor(m);
    if (si < 0) continue;
    const p = pickQuant(m);
    if (!p) {
      tooBig++;
      continue;
    }
    pool.push({ m, p, si, s: score(m) });
  }
  const byBase = new Map<string, Ranked>();
  for (const x of pool) {
    const cur = byBase.get(x.m.base);
    if (!cur || x.si < cur.si) byBase.set(x.m.base, x);
  }
  return { items: [...byBase.values()].sort((a, b) => b.s - a.s), tooBig, dupes: pool.length - byBase.size };
}

export function tplFit(t: Template): Fit {
  const v = vram();
  const ram = TIERS[S.tier].ram;
  if (v === 0) return t.size <= ram * 0.6 ? "cpu" : "none";
  if (t.size + 1.5 <= v) return "full";
  if (t.size <= v + ram * 0.5) return "partial";
  return "none";
}

/* ---- simulated downloads and server state ---- */

/** Sample flows (until phase 5) start through the real launcher with an empty config. */
export function startServer(label: string) {
  launch(label, sampleConfig(label));
}

export function stopServer() {
  stop();
}

export function startDownload(key: string, label: string, size: number) {
  if (S.downloads[key]) return;
  S.downloads[key] = { pct: 0, size };
  render();
  const tick = () => {
    const d = S.downloads[key];
    if (!d) return;
    d.pct = Math.min(100, d.pct + 4 + Math.random() * 7);
    if (d.pct >= 100) {
      delete S.downloads[key];
      startServer(label);
      return;
    }
    updateProgress(key);
    setTimeout(tick, 180);
  };
  setTimeout(tick, 180);
}

function cancelDownload(key: string) {
  delete S.downloads[key];
  render();
  toast("Download cancelled");
}

const dlText = (d: { pct: number; size: number }) => `Downloading ${gb((d.size * d.pct) / 100)} of ${gb(d.size)} · starts automatically`;

function updateProgress(key: string) {
  const el = document.querySelector<HTMLElement>(`[data-dl="${CSS.escape(key)}"]`);
  if (!el) return render();
  const d = S.downloads[key];
  el.querySelector<HTMLElement>(".bar-fill")!.style.width = d.pct + "%";
  el.querySelector("small")!.textContent = dlText(d);
}

/** Get-and-start button, or progress bar while downloading, or Stop while running. */
export function goControl(key: string, label: string, size: number, installed: boolean) {
  const d = S.downloads[key];
  if (d)
    return h("div", { class: "prog", "data-dl": key },
      h("div", { class: "go" },
        h("div", { class: "bar-track", style: "flex:1" }, h("div", { class: "bar-fill", style: `width:${d.pct}%` })),
        h("button", { onclick: () => cancelDownload(key) }, "Cancel")),
      h("small", {}, dlText(d)));
  if (S.running === label) return h("button", { onclick: stopServer }, "Stop");
  return h("button", { class: installed ? "" : "primary", onclick: () => (installed ? startServer(label) : startDownload(key, label, size)) },
    installed ? "Start" : "Get and start");
}
