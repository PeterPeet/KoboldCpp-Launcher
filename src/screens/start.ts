// Start screen: this PC's hardware, KoboldCpp templates, popular models from the user's sources.

import { api, fmtBytes, type Fit, type PopularItem, type Template } from "../api";
import { checkbox, fmt, h, toggle, toast } from "../dom";
import { openSources } from "../dialogs/sources";
import { getAndStart, goControl } from "../downloads";
import { S, machine, render, vram } from "../state";
import { serverBanner } from "./banner";

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
const estSpeed = (size: number, fit: Fit) => Math.max(1, Math.round((fit === "full" ? 520 : fit === "cpu" ? 60 : 110) / (size / GB + 2)));

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
  const key = JSON.stringify([S.family, S.sort, S.official, S.uncensored, machine(), S.sources.map((s) => [s.id, s.on, s.comps])]);
  if (key === S.popularKey) return;
  S.popularKey = key;
  S.popular = null;
  try {
    S.popular = await api.popular({ officialOnly: S.official, showUncensored: S.uncensored, sort: S.sort }, machine(), S.family);
    S.popularError = "";
  } catch (e) {
    S.popularError = String(e);
  }
  if (S.popularKey === key) render();
}

/** "Official" (maker's own account), "Quant by X", or "Finetune". */
export function kindPill(m: { kind: string; author: string }) {
  if (m.kind === "official") return h("span", { class: "pill p-ok", title: "Uploaded by the model maker" }, "Official");
  if (m.kind === "quant") return h("span", { class: "pill p-mute", title: "Unchanged model, quantized by someone else" }, `Quant by ${m.author}`);
  return h("span", { class: "pill p-acc", title: "A changed version of a model" }, "Finetune");
}

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

function startPopular(m: PopularItem) {
  const files = [{ url: m.urls[0], size: m.pick.size }];
  if (m.mmproj) files.push({ url: m.mmproj[0], size: m.mmproj[1] });
  const cfg: Record<string, unknown> = { model_param: m.urls[0], contextsize: 16384 };
  if (m.mmproj) cfg.mmproj = m.mmproj[0];
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
    h("span", { class: "hwchip on fixed", title: "The CPU is always used" }, `CPU · ${hw.cpuName || hw.arch} · ${fmtBytes(hw.ramTotal)} RAM`),
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
  const seg = h("div", { class: "seg", role: "group", "aria-label": "Which models" },
    ...([["Original models", true], ["With finetunes", false]] as const).map(([l, v]) =>
      h("button", { "aria-pressed": String(S.official === v), onclick: () => { S.official = v; render(); } }, l)));
  const famRow = h("div", { class: "famrow", role: "tablist", "aria-label": "Model family" },
    ...([[null, "Top", ""], ...families] as [string | null, string, string][]).map(([id, label, maker]) =>
      h("button", { role: "tab", "aria-selected": String(S.family === id), title: maker ? `Models from hf.co/${maker} and quants of them` : "The best model of each maker", onclick: () => { S.family = id; render(); } }, label)));
  const sortSel = h("select", { id: "sortSel", "aria-label": "Order" }, ...SORTS.map(([v, l]) => h("option", { value: v, selected: S.sort === v }, l))) as HTMLSelectElement;
  sortSel.addEventListener("change", () => { S.sort = sortSel.value as typeof S.sort; render(); });
  const head = h("div", { class: "sec-head" },
    h("div", {},
      h("h2", {}, "Choose from popular models yourself"),
      h("p", {}, S.family === null
        ? `Each maker's ${SORTS.find((s) => s[0] === S.sort)![1].toLowerCase()} model that fits this PC, from its own Hugging Face account where it has one. Pick a family to see more.`
        : "From your sources, in the order you chose. The best quality that fits is picked for you.")),
    h("div", { class: "controls" }, h("label", { class: "sortlbl" }, "Order", sortSel), seg, toggle("unc", S.uncensored, (v) => { S.uncensored = v; render(); }, "Show uncensored finetunes")));
  const out: HTMLElement[] = [head, famRow];
  const pop = S.popular;
  if (!pop) {
    out.push(h("div", { class: "list" }, h("div", { class: "empty" }, S.popularError ? `Could not reach Hugging Face: ${S.popularError}` : "Loading popular models…")));
    return out;
  }
  const list = h("div", { class: "list" });
  if (!pop.items.length) list.append(h("div", { class: "empty" }, S.family ? "Nothing from this family fits this PC with your sources." : "Nothing matches. Turn on more sources or include finetunes."));
  for (const m of pop.items) {
    const key = "m:" + m.id;
    const ctl = goControl(key, m.name, () => startPopular(m), { installed: m.downloaded });
    list.append(h("div", { class: "row" },
      h("div", { class: "mono-badge fam-" + famClass(m) }, m.name.slice(0, 2)),
      h("div", { class: "main" },
        h("div", { class: "name" }, m.name,
          S.family === null && m.family ? h("span", { class: "pill p-fam" }, families.find((f) => f[0] === m.family)?.[1] ?? m.family) : null,
          m.downloaded ? h("span", { class: "pill p-ok" }, "Downloaded") : null,
          kindPill(m),
          m.uncensored ? h("span", { class: "pill p-warn" }, "Uncensored") : null,
          ...(m.pick.fit !== "full" || m.pick.fallback ? fitPill(m.pick.fit, m.pick.fallback) : []),
          m.mmproj ? h("span", { class: "pill p-mute" }, "Sees images") : null),
        h("div", { class: "sub" }, [m.params, m.pick.quant, fmtBytes(m.pick.size), `about ${estSpeed(m.pick.size, m.pick.fit)} tokens/s`].filter(Boolean).join(" · ")),
        h("div", { class: "why" }, h("span", { class: "repo" }, m.id), ` · ${fmt(m.downloads)} downloads this month · ${fmt(m.likes)} likes · ${Math.round(m.ageDays)} days old`),
        S.dl[key] ? ctl : null),
      S.dl[key] ? null : ctl));
  }
  out.push(list);
  if (pop.errors.length) out.push(h("p", { class: "note", style: "margin:-10px 0 0" }, `Not reachable: ${pop.errors.join("; ")}`));
  if (!S.official && !S.sources.some((s) => s.on && s.comps && (s.comps.finetune || s.comps.unc)))
    out.push(h("p", { class: "note", style: "margin:-10px 0 0" },
      "Your sources only allow original models, so finetunes don't appear. ",
      h("button", { class: "linkbtn", onclick: openSources }, "Add bartowski or TheDrummer"), " to see finetunes."));
  const hidden = [pop.tooBig ? `${pop.tooBig} too big for this PC, hidden` : "", pop.dupes ? `${pop.dupes} duplicate upload${pop.dupes > 1 ? "s" : ""} merged` : ""].filter(Boolean).join(" · ");
  out.push(h("div", { class: "foot" },
    h("span", {}, hidden || "Nothing hidden"),
    h("span", {},
      h("button", { class: "linkbtn", onclick: openSources }, "Manage sources"), " · ",
      h("a", { href: "#library", style: "color:var(--accent-text)" }, "Model library"), " · ",
      h("a", { href: "#advanced", style: "color:var(--accent-text)" }, "Advanced setup"))));
  return out;
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
