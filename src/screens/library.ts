// Model library: local models as cards with filters, plus Hugging Face search and download.

import { api, fmtBytes, inTauri, type Fit, type LibraryView, type LocalModel, type RepoFiles, type SearchHit } from "../api";
import { $, checkbox, fmt, h, toast, type Kid } from "../dom";
import { getAndStart, goControl } from "../downloads";
import { launch, stop } from "../server";
import { S, machine, render } from "../state";
import { famClass, fitPill, kindPill } from "./start";

const KINDS = ["Text", "Vision", "Image", "Speech", "Embedding"] as const;
const KIND_LABEL: Record<string, string> = { Text: "Text", Vision: "Text + sees images", Image: "Image", Speech: "Speech", Embedding: "Embedding", Projector: "Vision add-on" };

let view: LibraryView | null = null;
let loading = false;
let fits: Record<string, Fit> = {};
let fitKey = "";
let search: { q: string; results: SearchHit[] | null; error: string; open: string | null; files: RepoFiles | null; seq: number } = { q: "", results: null, error: "", open: null, files: null, seq: 0 };
let searchTimer: number | undefined;

/** Forget the last scan; the next visit (or render while visible) scans again. */
export function invalidateLibrary() {
  view = null;
  fitKey = "";
}

export async function reloadLibrary() {
  loading = true;
  try {
    view = await api.library();
  } catch (e) {
    toast(`Could not read the model folders: ${e}`);
  }
  loading = false;
  fitKey = "";
  render();
}

async function loadFits() {
  if (!view) return;
  const key = JSON.stringify([machine(), view.models.length]);
  if (key === fitKey) return;
  fitKey = key;
  const ms = view.models;
  const r = await api.fits(ms.map((m) => [m.size + (m.mmproj ? 0.9 * 1024 ** 3 : 0), Math.min(m.contextLength ?? 16384, 16384)]), machine());
  fits = Object.fromEntries(ms.map((m, i) => [m.path, r[i]]));
  renderLibGrid();
}

const family = (m: LocalModel) => (m.architecture ?? "other").replace(/\d.*$/, "") || "other";
const ago = (t: number | null) => {
  if (!t) return "Never";
  const d = Math.floor((Date.now() / 1000 - t) / 86400);
  return d <= 0 ? "Today" : d === 1 ? "Yesterday" : d < 30 ? `${d} days ago` : new Date(t * 1000).toLocaleDateString();
};

async function pickFolder(): Promise<string | null> {
  // Browser preview: a sample path (a typical wrong pick, one level too deep).
  if (!inTauri) return "/Users/you/.lmstudio/models/lmstudio-community/Qwen3.5-4B-GGUF";
  const { open } = await import("@tauri-apps/plugin-dialog");
  const p = await open({ directory: true, multiple: false });
  return typeof p === "string" ? p : null;
}

/** Parent of a path ("/a/b/c/" → "/a/b"), or null at the top. */
function parentOf(path: string): string | null {
  const trimmed = path.replace(/[\\/]+$/, "");
  const i = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  return i > 0 ? trimmed.slice(0, i) : null;
}

/** Add-folder dialog: pick a folder, choose whether subfolders are searched too, see what it finds. */
function addFolder() {
  const root = $("#dialogRoot")!;
  let path = "";
  let recursive = true;
  let found: { models: number; folders: number } | null = null;
  let seq = 0;
  const close = () => root.replaceChildren();

  const check = async () => {
    found = null;
    draw();
    if (!path) return;
    const mine = ++seq;
    try {
      const r = await api.previewFolder(path, recursive);
      if (mine === seq) { found = r; draw(); }
    } catch { /* leave empty */ }
  };

  function draw() {
    const up = path ? parentOf(path) : null;
    const summary = !path ? null
      : found === null ? h("p", { class: "note", style: "margin:0" }, "Looking for models…")
      : found.models === 0
        ? h("p", { class: "err", style: "margin:0" }, recursive
            ? "No .gguf models in this folder or its subfolders. Maybe one level up?"
            : "No .gguf models directly in this folder. Turn on “Add recursively” or go one level up.")
        : h("p", { class: "ok-note" }, `${found.models} model${found.models > 1 ? "s" : ""}` + (found.folders > 1 ? ` in ${found.folders} folders` : "") + " found.");
    const dlg = h("div", { class: "dialog", role: "dialog", "aria-modal": "true", "aria-labelledby": "addTitle", style: "width:min(560px,100%)" },
      h("div", {},
        h("h2", { id: "addTitle" }, "Add folder"),
        h("p", {}, "The library shows every .gguf model in this folder. Models stay where they are.")),
      // The full path, wrapped over several lines; the buttons get their own row below it.
      h("div", { class: "pathbox mono" }, path || "No folder chosen"),
      h("div", { style: "display:flex;gap:8px;flex-wrap:wrap" },
        h("button", { onclick: async () => { const p = await pickFolder(); if (p) { path = p; check(); } } }, path ? "Choose another…" : "Choose folder…"),
        path ? h("button", { disabled: !up, title: up ?? "", onclick: () => { if (up) { path = up; check(); } } }, "↑ One level up") : null),
      summary,
      h("label", { class: "chk", for: "addRecursive", style: "font-size:14px" },
        checkbox("addRecursive", recursive, (v) => { recursive = v; check(); }),
        "Add recursively from the folder"),
      h("p", { class: "note", style: "margin:-6px 0 0 23px" }, "Also search all subfolders, e.g. LM Studio's models folder, which keeps each model in its own folder."),
      h("div", { class: "btns" },
        h("button", { onclick: close }, "Cancel"),
        h("button", { class: "primary", disabled: !path, onclick: async () => {
          try {
            S.settings = await api.addFolder(path, recursive);
            close();
            toast("Folder added");
            reloadLibrary();
          } catch (e) {
            toast(String(e));
          }
        } }, "Add folder")));
    const scrim = h("div", { class: "scrim", onclick: (e: Event) => { if (e.target === scrim) close(); } }, dlg);
    root.replaceChildren(scrim);
  }
  draw();
}

function startModel(m: LocalModel) {
  const cfg: Record<string, unknown> = { model_param: m.path, contextsize: Math.min(m.contextLength ?? 16384, 16384) };
  if (m.kind === "Image") Object.assign(cfg, { model_param: "", sdmodel: m.path });
  if (m.kind === "Speech") Object.assign(cfg, { model_param: "", [m.file.toLowerCase().includes("whisper") ? "whispermodel" : "ttsmodel"]: m.path });
  if (m.kind === "Embedding") Object.assign(cfg, { model_param: "", embeddingsmodel: m.path });
  if (m.mmproj) cfg.mmproj = m.mmproj;
  const go = async () => { if (S.running) await stop(); launch(m.name, cfg); };
  go();
}

async function moveModel(m: LocalModel) {
  const dest = await pickFolder();
  if (!dest) return;
  toast(`Moving ${m.name}…`);
  try {
    await api.moveModel(m.path, dest);
    toast(`${m.name} moved`);
    if (S.settings && !S.settings.libraryFolders.some((f) => dest.startsWith(f.path)) && !dest.startsWith(view?.modelsDir ?? "\0")) S.settings = await api.addFolder(dest, false);
    reloadLibrary();
  } catch (e) {
    toast(String(e));
  }
}

export function renderLibrary() {
  if (!view && !loading) reloadLibrary();
  const L = S.lib;
  const root = h("div", { class: "lib" });
  const side = h("aside", { class: "side" });

  if (L.tab === "local") {
    const q = h("input", { type: "search", id: "libq", placeholder: "Filter models", value: L.q, "aria-label": "Filter models" }) as HTMLInputElement;
    q.addEventListener("input", () => { L.q = q.value; renderLibGrid(); });
    side.append(q);
  } else {
    side.append(h("p", { class: "note", style: "margin:0" }, "Search all GGUF models on Hugging Face. Sizes shown are for Q4_K_M, the usual choice."));
  }

  if (view && L.tab === "local") {
    const models = view.models.filter((m) => m.kind !== "Projector");
    const facet = (title: string, key: "types" | "fams", items: [string, string, number, Kid?][]) => {
      const box = h("div", { style: "display:grid;grid-template-columns:minmax(0,1fr);gap:2px" }, h("h3", { style: "margin-bottom:6px" }, title));
      for (const [val, label, n, badge] of items) {
        const id = `f-${key}-${val}`;
        box.append(h("label", { class: "chk", for: id }, badge, label, h("span", { class: "n" }, n),
          checkbox(id, L[key].has(val), () => { L[key].has(val) ? L[key].delete(val) : L[key].add(val); renderLibGrid(); })));
      }
      return box;
    };
    side.append(facet("Type", "types", KINDS.map((k) => [k, KIND_LABEL[k], models.filter((m) => m.kind === k).length] as [string, string, number]).filter((x) => x[2] > 0)));
    const fams = [...new Set(models.map(family))].sort();
    side.append(facet("Family", "fams", fams.map((f) => [f, f, models.filter((m) => family(m) === f).length, h("span", { class: `mono-badge fam-${famClass({ name: f })}` }, f.slice(0, 2))])));

    const dbox = h("div", { style: "display:grid;grid-template-columns:minmax(0,1fr);gap:6px" }, h("h3", {}, "Drive"));
    for (const d of view.drives) {
      const used = models.filter((m) => m.drive === d.mount).reduce((a, m) => a + m.size, 0);
      const pct = 100 - (d.free / d.total) * 100;
      const id = "f-drive-" + d.mount;
      dbox.append(h("label", { class: "drive", for: id },
        h("span", { class: "chk", style: "padding:0" }, d.mount === "/" ? "System disk" : d.mount.split(/[\\/]/).filter(Boolean).pop() ?? d.mount, h("span", { class: "n" }),
          checkbox(id, L.drives.has(d.mount), () => { L.drives.has(d.mount) ? L.drives.delete(d.mount) : L.drives.add(d.mount); renderLibGrid(); })),
        h("div", { class: "bar-track" }, h("div", { class: "bar-fill", style: `width:${pct}%;background:${pct > 85 ? "var(--warn)" : "var(--fg-2)"}` })),
        h("small", {}, `${fmtBytes(used)} of models · ${fmtBytes(d.free)} free`)));
    }
    side.append(dbox);

    const fbox = h("div", { style: "display:grid;grid-template-columns:minmax(0,1fr);gap:4px" }, h("h3", { style: "margin-bottom:4px" }, "Folders"));
    for (const f of view.folders) {
      const own = f.path === view.modelsDir;
      fbox.append(h("div", { class: "folder", title: `${f.path}${f.recursive ? " (with subfolders)" : " (this folder only)"}` },
        h("span", { class: "mono" }, f.path.split(/[\\/]/).slice(-2).join("/")),
        own ? h("span", { class: "n" }, "downloads")
          : h("span", { style: "display:inline-flex;gap:6px;align-items:center;flex-shrink:0" },
              f.recursive ? h("span", { class: "n", title: "Subfolders are searched too" }, "+ sub") : null,
              h("button", { class: "linkbtn", onclick: async () => { S.settings = await api.removeFolder(f.path); reloadLibrary(); } }, "Remove"))));
    }
    side.append(fbox);
  }

  const main = h("section", { class: "libmain" });
  const tabs = h("div", { class: "tabs", role: "tablist" },
    ...([["local", "On this computer"], ["discover", "Discover on Hugging Face"]] as const).map(([k, l]) =>
      h("button", { role: "tab", "aria-selected": String(L.tab === k), onclick: () => { L.tab = k; render(); } }, l)));
  main.append(h("div", { style: "display:flex;justify-content:space-between;align-items:flex-end;gap:10px;flex-wrap:wrap" },
    tabs,
    h("div", { style: "display:flex;gap:6px" },
      h("button", { class: "ghost", onclick: reloadLibrary }, "Rescan"),
      h("button", { onclick: addFolder }, "Add folder"))));
  main.append(h("div", { id: "libBody" }));
  root.append(side, main);
  setTimeout(renderLibGrid);
  return root;
}

/** Search as you type (short pause first); an empty field clears the list. */
function onSearchInput(value: string) {
  search.q = value;
  clearTimeout(searchTimer);
  if (value.trim().length < 2) {
    search.seq++;
    search.results = null;
    search.error = "";
    search.open = null;
    renderLibGrid();
    return;
  }
  searchTimer = window.setTimeout(runSearch, 350);
}

async function runSearch() {
  const q = search.q.trim();
  if (q.length < 2) return;
  const seq = ++search.seq;
  search.error = "";
  search.open = null;
  if (search.results === null) renderLibGrid();
  try {
    const r = await api.search(q);
    if (seq !== search.seq) return;
    search.results = r;
  } catch (e) {
    if (seq !== search.seq) return;
    search.error = String(e);
    search.results = [];
  }
  renderLibGrid();
}

async function openRepo(id: string) {
  search.open = search.open === id ? null : id;
  search.files = null;
  renderLibGrid();
  if (!search.open) return;
  try {
    search.files = await api.repoFiles(id);
  } catch (e) {
    toast(String(e));
  }
  renderLibGrid();
}

function repoFilesView(r: SearchHit) {
  const f = search.files;
  if (!f) return h("div", { class: "note", style: "margin:6px 0 0" }, "Loading files…");
  if (!f.quants.length) return h("div", { class: "note", style: "margin:6px 0 0" }, "No GGUF model files in this repo.");
  const mm = f.mmproj.find(([p]) => /f16/i.test(p)) ?? f.mmproj[0];
  const box = h("div", { class: "quants" });
  for (const q of [...f.quants].sort((a, b) => a.size - b.size)) {
    const key = `d:${r.id}:${q.label}`;
    const files = [{ url: `https://huggingface.co/${r.id}/resolve/main/${q.files[0]}`, size: q.size }];
    if (mm) files.push({ url: `https://huggingface.co/${r.id}/resolve/main/${mm[0]}`, size: mm[1] });
    box.append(h("div", { class: "qrow" },
      h("span", { class: "mono" }, q.label), h("span", {}, fmtBytes(q.size) + (q.files.length > 1 ? ` · ${q.files.length} parts` : "")),
      goControl(key, `${r.name} ${q.label}`, () => getAndStart(key, `${r.name}`, files, { model_param: files[0].url, ...(mm ? { mmproj: files[1].url } : {}) }).then(reloadLibrary), { text: "Get and start" })));
  }
  return box;
}

function renderLibGrid() {
  const L = S.lib;
  const body = $("#libBody");
  if (!body) return;

  if (L.tab === "discover") {
    // The search field lives outside the results so typing never loses focus.
    let input = $<HTMLInputElement>("#hfq");
    if (!input || !body.contains(input)) {
      input = h("input", { id: "hfq", type: "search", placeholder: "Model name, e.g. gemma 4 e2b", value: search.q, "aria-label": "Search Hugging Face", autocomplete: "off" }) as HTMLInputElement;
      input.addEventListener("input", () => onSearchInput(input!.value));
      input.addEventListener("keydown", (e) => { if (e.key === "Enter") { clearTimeout(searchTimer); runSearch(); } });
      body.replaceChildren(h("div", { class: "addrow" }, input), h("div", { id: "hfResults", style: "display:grid;gap:10px" }));
      setTimeout(() => input?.focus());
    }
    const results = $("#hfResults")!;
    const list = h("div", { class: "list" });
    for (const r of search.results ?? []) {
      list.append(h("div", { class: "row", style: "align-items:flex-start" },
        h("div", { class: `mono-badge fam-${famClass(r)}` }, r.name.slice(0, 2)),
        h("div", { class: "main" },
          h("div", { class: "name" }, r.name, kindPill(r), r.uncensored ? h("span", { class: "pill p-warn" }, "Uncensored") : null),
          h("div", { class: "repo" }, r.id),
          h("div", { class: "why" }, `${fmt(r.downloads)} downloads this month · ${fmt(r.likes)} likes · ${Math.round(r.ageDays)} days old`),
          search.open === r.id ? repoFilesView(r) : null),
        h("span", { class: "typ", title: r.typical ? `Size of the ${r.typical[0]} file` : "No common quant found" }, r.typical ? `${r.typical[0]} · ${fmtBytes(r.typical[1])}` : "–"),
        h("button", { onclick: () => openRepo(r.id) }, search.open === r.id ? "Close" : "Choose file")));
    }
    const typing = search.q.trim().length >= 2;
    results.replaceChildren(
      search.results === null
        ? (typing ? h("div", { class: "empty" }, "Searching…") : h("div", { class: "empty" }, "Type a model name to search."))
        : search.error ? h("div", { class: "empty" }, search.error)
        : search.results.length ? list : h("div", { class: "empty" }, "No GGUF models found."));
    return;
  }

  if (!view) {
    body.replaceChildren(h("div", { class: "empty" }, "Reading your model folders…"));
    return;
  }
  loadFits();
  const qq = L.q.toLowerCase();
  const items = view.models.filter((m) =>
    m.kind !== "Projector" &&
    (!L.types.size || L.types.has(m.kind)) &&
    (!L.fams.size || L.fams.has(family(m))) &&
    (!L.drives.size || (m.drive && L.drives.has(m.drive))) &&
    (!qq || (m.name + m.file).toLowerCase().includes(qq)));
  const total = items.reduce((a, m) => a + m.size, 0);
  const grid = h("div", { class: "grid" });
  const kv = (k: string, v: Kid, style?: string) => h("div", { class: "kv" }, h("span", {}, k), h("span", { style }, v));

  for (const m of items) {
    const acts = h("div", { class: "acts" });
    const running = S.running === m.name && S.server.state !== "stopped";
    if (L.confirm === m.path)
      acts.append(h("div", { class: "confirm" }, `Move ${fmtBytes(m.size)} to the Trash?`,
        h("button", { onclick: async () => { L.confirm = null; try { await api.trashModel(m.path); toast(`${m.name} moved to the Trash`); } catch (e) { toast(String(e)); } reloadLibrary(); } }, "Move to Trash"),
        h("button", { class: "ghost", onclick: () => { L.confirm = null; renderLibGrid(); } }, "Keep")));
    else
      acts.append(
        h("button", { class: running ? "" : "go-start", onclick: () => (running ? stop() : startModel(m)) }, running ? "Stop" : "Start"),
        h("button", { onclick: () => moveModel(m) }, "Move…"),
        h("button", { class: "ghost", onclick: () => { L.confirm = m.path; renderLibGrid(); } }, "Remove"));
    const f = fits[m.path];
    grid.append(h("article", { class: "card" },
      h("div", { class: "top" }, h("span", { class: `mono-badge fam-${famClass(m)}`, style: "width:30px;height:30px;font-size:12px" }, m.name.slice(0, 2)), ...(f ? fitPill(f) : [])),
      h("h4", {}, m.name),
      h("div", { class: "tags" }, [m.sizeLabel, m.quant, m.architecture, m.parts > 1 ? `${m.parts} parts` : null].filter(Boolean).join(" · ")),
      kv("Type", KIND_LABEL[m.kind]), kv("Size", fmtBytes(m.size)),
      kv("Context", m.contextLength ? `${Math.round(m.contextLength / 1024)}K` : "–"),
      kv("Layers", m.layers ?? "–"),
      kv("Drive", m.drive === "/" ? "System disk" : m.drive ?? "–"),
      kv("Last used", ago(m.lastUsed)),
      acts));
  }
  const empty = view.models.length === 0
    ? h("div", { class: "empty" }, "No models yet. Start one from the start screen, download from Hugging Face, or add a folder that has .gguf files.")
    : h("div", { class: "empty" }, "No models match these filters.");
  body.replaceChildren(h("div", { style: "display:grid;gap:10px" },
    h("div", { class: "note", style: "margin:0" }, `${items.length} models · ${fmtBytes(total)}`),
    items.length ? grid : empty));
}
