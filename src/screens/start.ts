// Start screen: this PC's hardware, KoboldCpp templates, popular models from the user's sources.

import { api, fmtBytes, type Fit, type PopularItem, type Template } from "../api";
import { checkbox, fmt, h, toggle, toast } from "../dom";
import { openSources } from "../dialogs/sources";
import { getAndStart, goControl } from "../downloads";
import { S, machine, render, vram } from "../state";
import { serverBanner } from "./banner";
import { getOpts, setOpts } from "../modelopts";

const GB = 1024 ** 3;
const TIER_LABEL = { low: "Low spec", mid: "Mid spec", high: "High spec" } as const;
/** Template tier for the selected GPUs (KoboldCpp: Low 6 GB, Mid 12 GB, High 24 GB). */
const selectedTier = () => (vram() >= 22 * GB ? "high" : vram() >= 11 * GB ? "mid" : "low");

export function fitPill(f: Fit, fallback = false) {
  const p = (cls: string, t: string) => h("span", { class: `pill ${cls}` }, t);
  const base = f === "full" ? p("p-ok", "Fits") : f === "partial" ? p("p-warn", "Partly on CPU") : f === "cpu" ? p("p-mute", "Runs on CPU") : p("p-bad", "Too big");
  return fallback ? [base, p("p-warn", "Smaller quant")] : [base];
}

/** Rough speed for the list (tokens/s); measured speeds replace this later. */
/** "13.3 tokens/s measured" when KoboldCpp ran it here, else "about 18 tokens/s", or N/A when
 *  a memory bandwidth it needs is unknown. */
export const speedText = (tps: number | null, measured: number | null) =>
  measured ? `${measured.toFixed(1)} tokens/s measured`
  : tps == null ? "speed N/A"
  : `about ${tps >= 10 ? Math.round(tps) : tps.toFixed(1)} tokens/s`;

/* ---------- data loading ---------- */

let tplRequested = false;
let tplFits: Record<string, Fit> = {};
let tplFitKey = "";

async function loadTemplates() {
  try {
    S.templates = await api.templates();
    S.templatesError = "";
  } catch (e) {
    S.templatesError = String(e);
    S.templates = [];
  }
  render();
}

async function loadTemplateFits() {
  const key = JSON.stringify([machine(), S.templates?.length]);
  if (key === tplFitKey || !S.templates) return;
  tplFitKey = key;
  const list = S.templates;
  const fits = await api.fits(list.map((t) => [t.size ?? 0, t.context ?? 8192]), machine());
  tplFits = Object.fromEntries(list.map((t, i) => [t.id, fits[i]]));
  render();
}

let families: [string, string, string][] = [];
const SORTS: [string, string][] = [["downloads", "Most downloaded"], ["trending", "Trending"], ["likes", "Most liked"], ["newest", "Newest"]];
async function loadFamilies() {
  try { families = await api.families(); } catch { families = []; }
  render();
}

async function loadPopular() {
  const key = JSON.stringify([S.family, S.sort, machine(), S.sources.map((s) => [s.id, s.on, s.comps])]);
  if (key === S.popularKey) return;
  S.popularKey = key;
  S.popular = null;
  try {
    // What appears is decided by the model sources (their Official / Finetunes / Uncensored boxes).
    S.popular = await api.popular({ officialOnly: false, showUncensored: true, sort: S.sort }, machine(), S.family);
    S.popularError = "";
  } catch (e) {
    S.popularError = String(e);
  }
  if (S.popularKey === key) render();
}

/** "Official" (the maker's own account) or "Quant by <uploader>". */
export function kindPill(m: { kind: string; author: string }) {
  if (m.kind === "official") return h("span", { class: "pill p-ok", title: "Uploaded by the model maker" }, "Official");
  return h("span", { class: "pill p-mute", title: `Uploaded by ${m.author}` }, `Quant by ${m.author}`);
}

/** Everything this row would start with is on disk (depends on the Vision choice). */
const isDownloaded = (m: PopularItem) => m.modelDownloaded && (!m.mmproj || !getOpts(m.id).vision || m.mmprojDownloaded);

/* ---------- actions ---------- */

/** GPU selection → backend for this run. */
function backendFor(cfg: Record<string, unknown>) {
  const hw = S.hw;
  if (!hw) return cfg;
  const chosen = hw.gpus.filter((_, i) => S.useGpu[i] !== false);
  if (hw.gpus.length && !chosen.length) return { ...cfg, usecpu: true, usecuda: null, usevulkan: null };
  if (chosen.length && chosen.length < hw.gpus.length) {
    const ids = chosen.map((g) => g.index);
    if (chosen[0].api === "cuda" || chosen[0].api === "rocm") return { ...cfg, usecuda: ["normal", String(ids[0])], usevulkan: null, usecpu: false };
    if (chosen[0].api === "vulkan") return { ...cfg, usevulkan: ids, usecuda: null, usecpu: false };
  }
  return cfg;
}

function startTemplate(t: Template) {
  const files = t.files.map(([, url, size]) => ({ url, size }));
  const cfg = backendFor({ ...t.config, istemplate: false });
  getAndStart("t:" + t.id, t.title, files, cfg);
}

export function startPopular(m: PopularItem) {
  const vision = !!m.mmproj && getOpts(m.id).vision;
  const files = [{ url: m.urls[0], size: m.pick.size }];
  if (vision) files.push({ url: m.mmproj![0], size: m.mmproj![1] });
  const cfg: Record<string, unknown> = { model_param: m.urls[0], contextsize: 16384 };
  if (vision) cfg.mmproj = m.mmproj![0];
  if (m.mtp && getOpts(m.id).mtp) cfg.usemtp = true;
  getAndStart("m:" + m.id, m.name, files, backendFor(cfg));
}

/* ---------- view ---------- */

function hardwareRow() {
  const hw = S.hw;
  if (!hw) return h("div", { class: "hw" }, h("span", { class: "spinner", "aria-hidden": "true" }), "Detecting your hardware…");
  const chips = hw.gpus.map((g, i) =>
    h("label", { class: "hwchip" + (S.useGpu[i] !== false ? " on" : ""), for: "gpu-" + i },
      checkbox("gpu-" + i, S.useGpu[i] !== false, (v) => {
        S.useGpu[i] = v;
        render();
        toast(vram() ? `Using ${fmtBytes(vram())} ${hw.unifiedMemory ? "of shared memory for the GPU" : "graphics memory"}` : "CPU only: slower, but runs without a GPU");
      }),
      `${g.name} · ${fmtBytes(g.vram)}${hw.unifiedMemory ? " usable" : ""}`));
  const tier = vram() ? `${TIER_LABEL[selectedTier()]} · ${fmtBytes(vram())} ${hw.unifiedMemory ? "for models" : "GPU"}` : "CPU only";
  return h("div", { class: "hw" },
    h("span", {}, "Run on"), ...chips,
    // "for models": what the launcher counts on when a model runs on the CPU (60 % of RAM, as in fit.rs).
    h("span", { class: "hwchip on fixed", title: `The CPU is always used. For system stability, models should use at most ${fmtBytes(hw.ramTotal * 0.6)}, which leaves room for the system and other apps. Memory bandwidth: ${hw.ramBandwidth ? `about ${Math.round(hw.ramBandwidth)} GB/s` : "N/A (the system doesn't report the memory type and speed)"}.` },
      `CPU · ${hw.cpuName || hw.arch} · ${fmtBytes(hw.ramTotal)} ${hw.ramType ? hw.ramType + " RAM" : "RAM (type N/A)"} · ~${fmtBytes(hw.ramTotal * 0.6)} for models`),
    h("span", { class: "pill p-acc" }, tier),
    hw.gpus.length === 0 ? h("span", { class: "note", style: "margin:0" }, "No supported GPU found. KoboldCpp runs on the CPU.") : null);
}

function templatesSection() {
  const ks = S.sources.find((s) => s.id === "kcppt");
  if (ks && !ks.on) return [];
  const tier = selectedTier();
  const head = h("div", { class: "sec-head" },
    h("div", {},
      h("h2", {}, "KoboldCpp's ready-made setups"),
      h("p", {}, `KoboldCpp's own templates for ${TIER_LABEL[tier].toLowerCase()} PCs. One click downloads every model in it.`)),
    toggle("allTiers", S.showAllTiers, (v) => { S.showAllTiers = v; render(); }, "Show all templates"));
  if (!S.templates) return [head, h("div", { class: "empty" }, "Loading KoboldCpp's templates…")];
  if (S.templatesError && !S.templates.length) return [head, h("div", { class: "empty" }, `Could not load templates: ${S.templatesError}`)];
  loadTemplateFits();
  const list = S.templates
    .filter((t) => S.showAllTiers || t.tier === tier)
    .map((t) => ({ t, f: tplFits[t.id] ?? ("full" as Fit) }))
    .filter((o) => S.showAllTiers || o.f !== "none");
  const tiles = h("div", { class: "tiles" });
  list.forEach(({ t, f }, i) =>
    tiles.append(h("div", { class: "tile" + (i === 0 && !S.showAllTiers ? " best" : "") },
      h("div", { class: "tile-head" }, h("div", {}, h("h4", {}, t.title), h("div", { class: "file" }, t.id + ".kcppt")), ...fitPill(f)),
      h("div", { class: "parts" }, ...t.parts.map((p) => h("span", {}, p))),
      h("div", { class: "meta" }, (t.size ? `${fmtBytes(t.size)} download` : "Size unknown") + (t.context ? ` · ${Math.round(t.context / 1024)}K context` : "")),
      f === "none" ? h("div", { class: "meta", style: "color:var(--bad)" }, "Needs more memory than this PC has") : goControl("t:" + t.id, t.title, () => startTemplate(t), { installed: t.downloaded }))));
  return [head, list.length ? tiles : h("div", { class: "empty" }, "No templates for this PC. Turn on “Show all templates”.")];
}

function popularSection() {
  loadPopular();
  if (!families.length) loadFamilies();
  const sortSel = h("select", { id: "sortSel", "aria-label": "Hugging Face sorting" }, ...SORTS.map(([v, l]) => h("option", { value: v, selected: S.sort === v }, l))) as HTMLSelectElement;
  sortSel.addEventListener("change", () => { S.sort = sortSel.value as typeof S.sort; render(); });
  const famRow = h("div", { class: "famrow", role: "tablist", "aria-label": "Model family" },
    ...([[null, "Top", ""], ...families] as [string | null, string, string][]).map(([id, label, maker]) =>
      h("button", { role: "tab", "aria-selected": String(S.family === id), title: maker ? `Models from hf.co/${maker} and quants of them` : "The best model of each maker", onclick: () => { S.family = id; render(); } }, label)),
    h("label", { class: "sortlbl" }, "Hugging Face sorting:", sortSel));
  const head = h("div", { class: "sec-head" },
    h("div", {},
      h("h2", {}, "Choose from popular models yourself"),
      h("p", {}, S.family === null
        ? `Model source: huggingface.co. Each maker's ${SORTS.find((s) => s[0] === S.sort)![1].toLowerCase()} model that fits this PC.`
        : "Model source: huggingface.co")));
  const out: HTMLElement[] = [head, famRow];
  const pop = S.popular;
  if (!pop) {
    out.push(h("div", { class: "list" }, h("div", { class: "empty" }, S.popularError ? `Could not reach Hugging Face: ${S.popularError}` : "Loading popular models…")));
    return out;
  }
  const list = h("div", { class: "list" });
  if (!pop.items.length) list.append(h("div", { class: "empty" }, S.family ? "Nothing from this family fits this PC with your model sources." : "Nothing fits. Check that the makers are on in Model Sources."));
  for (const m of pop.items) list.append(popularRow(m, S.family === null));
  out.push(list);
  if (pop.errors.length) out.push(h("p", { class: "note", style: "margin:-10px 0 0" }, `Not reachable: ${pop.errors.join("; ")}`));
  const hidden = [pop.tooBig ? `${pop.tooBig} too big for this PC, hidden` : "", pop.dupes ? `${pop.dupes} duplicate upload${pop.dupes > 1 ? "s" : ""} merged` : ""].filter(Boolean).join(" · ");
  out.push(h("div", { class: "foot" },
    h("span", {}, hidden || "Nothing hidden"),
    h("span", {},
      h("button", { class: "linkbtn", onclick: openSources }, "Model sources"), " · ",
      h("a", { href: "#library", style: "color:var(--accent-text)" }, "Model library"), " · ",
      h("a", { href: "#advanced", style: "color:var(--accent-text)" }, "Advanced setup"))));
  return out;
}

/** One model row, the same on the start screen and in Discover on Hugging Face:
 *  name, labels, Vision/MTP options, quant, size, speed, Start or Get and start. */
export function popularRow(m: PopularItem, showFamily = false) {
  const key = "m:" + m.id;
  const ctl = goControl(key, m.name, () => startPopular(m), { installed: isDownloaded(m) });
  // Switching an option on can need files that aren't here yet: the button follows.
  const opt = (id: string, label: string, title: string, on: boolean, set: (v: boolean) => void) =>
    h("label", { class: "pill p-mute opt", for: id, title }, checkbox(id, on, (v) => { set(v); render(); }), label);
  return h("div", { class: "row" },
    h("div", { class: "mono-badge fam-" + famClass(m) }, m.name.slice(0, 2)),
    h("div", { class: "main" },
      h("div", { class: "name" }, m.name,
        showFamily && m.family ? h("span", { class: "pill p-fam" }, families.find((f) => f[0] === m.family)?.[1] ?? m.family) : null,
        isDownloaded(m) ? h("span", { class: "pill p-ok" }, "Downloaded") : null,
        kindPill(m),
        m.uncensored ? h("span", { class: "pill p-warn" }, "Uncensored") : null,
        ...(m.pick.fit !== "full" || m.pick.fallback ? fitPill(m.pick.fit, m.pick.fallback) : []),
        m.mmproj ? opt(`vis-${m.id}`, "Vision", "Also download and load the vision file so the model can see images", getOpts(m.id).vision, (v) => setOpts(m.id, { vision: v })) : null,
        m.mtp ? opt(`mtp-${m.id}`, "MTP", "Use the model's MTP layers for faster answers (needs a bit more memory)", getOpts(m.id).mtp, (v) => setOpts(m.id, { mtp: v })) : null),
      h("div", { class: "sub", title: m.measured ? "Measured by KoboldCpp on this computer" : "Estimated from this computer's memory bandwidth and the model size" },
        // "Q4_0: 3.1 GB · Vision: 0.9 GB": what the download adds up to with the ticked options.
        [m.params, `${m.pick.quant}: ${fmtBytes(m.pick.size)}`, m.mmproj && getOpts(m.id).vision ? `Vision: ${fmtBytes(m.mmproj[1])}` : "", speedText(m.tps, m.measured)].filter(Boolean).join(" · ")),
      h("div", { class: "why" }, h("span", { class: "repo" }, m.id), ` · ${fmt(m.downloads)} downloads this month · ${fmt(m.likes)} likes · ${Math.round(m.ageDays)} days old`),
      S.dl[key] ? ctl : null),
    S.dl[key] ? null : ctl);
}

/** Letter-badge colour by model family (no logos). */
export function famClass(m: { name: string; id?: string }) {
  const n = (m.id ?? m.name).toLowerCase();
  for (const [k, c] of [["qwen", "qw"], ["mistral", "mi"], ["cydonia", "mi"], ["rocinante", "mi"], ["gemma", "ge"], ["lfm", "lf"], ["llama", "ll"], ["deepseek", "ll"], ["flux", "fl"], ["kokoro", "ko"], ["whisper", "wh"], ["bge", "bg"]] as const)
    if (n.includes(k)) return c;
  return "ll";
}

export function renderStart() {
  if (!tplRequested) {
    tplRequested = true;
    loadTemplates();
  }
  const root = h("div", { class: "start" });
  root.append(...serverBanner());
  root.append(h("div", { style: "display:grid;gap:8px" }, h("h1", {}, "Pick your hardware setup to start"), hardwareRow()));
  if (S.hw) root.append(...templatesSection(), ...popularSection());
  return root;
}
