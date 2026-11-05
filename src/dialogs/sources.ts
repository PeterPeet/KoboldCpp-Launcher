// Sources dialog: APT-style list of Hugging Face uploaders with components and priority.

import { api, inTauri } from "../api";
import { $, h, toggle, toast } from "../dom";
import { S, render } from "../state";

let saveTimer: number | undefined;
/** Save the list (debounced) and refresh the popular list. */
function persist() {
  S.popularKey = "";
  clearTimeout(saveTimer);
  saveTimer = window.setTimeout(async () => {
    if (!S.settings) return;
    try {
      S.settings = await api.saveSettings({ sources: S.sources });
    } catch (e) {
      toast(`Could not save sources: ${e}`);
    }
  }, 300);
}

async function exportSources() {
  if (!inTauri) return toast("Export works in the desktop app");
  const { save } = await import("@tauri-apps/plugin-dialog");
  const path = await save({ defaultPath: "sources.json", filters: [{ name: "Sources", extensions: ["json"] }] });
  if (!path) return;
  await api.exportSources(path).then(() => toast("Sources exported"), (e) => toast(String(e)));
}

async function importSources(redraw: () => void) {
  if (!inTauri) return toast("Import works in the desktop app");
  const { open } = await import("@tauri-apps/plugin-dialog");
  const path = await open({ multiple: false, filters: [{ name: "Sources", extensions: ["json"] }] });
  if (typeof path !== "string") return;
  try {
    S.sources = await api.importSources(path);
    if (S.settings) S.settings.sources = S.sources;
    S.popularKey = "";
    redraw();
    toast("Sources imported");
  } catch (e) {
    toast(String(e));
  }
}

const NAME_RE = /^[A-Za-z0-9][A-Za-z0-9_.-]{1,95}$/;

export function openSources() {
  const root = $("#dialogRoot")!;
  let err = "";
  const close = () => { root.replaceChildren(); render(); };
  const changed = () => { persist(); draw(); };

  const draw = () => {
    const list = h("div", { style: "border:1px solid var(--line);border-radius:12px" });
    S.sources.forEach((s, i) => {
      const mid = h("div", {},
        h("div", { class: "nm" }, s.name, s.builtin ? h("span", { class: "pill p-mute", style: "margin-left:6px" }, "Built in") : null),
        h("div", { class: "ds" }, s.desc));
      const swap = (j: number) => { [S.sources[i], S.sources[j]] = [S.sources[j], S.sources[i]]; changed(); };
      const ord = h("div", { class: "ord" },
        h("button", { "aria-label": `Move ${s.name} up`, disabled: i === 0, onclick: () => swap(i - 1) }, "▲"),
        h("button", { "aria-label": `Move ${s.name} down`, disabled: i === S.sources.length - 1, onclick: () => swap(i + 1) }, "▼"));
      const right = h("div", { style: "display:flex;gap:8px;align-items:center" },
        s.builtin ? null : h("button", { class: "ghost", "aria-label": `Remove ${s.name}`, onclick: () => { S.sources.splice(i, 1); changed(); } }, "Remove"),
        ord);
      list.append(h("div", { class: "src" }, toggle(`src-${s.id}`, s.on, (v) => { s.on = v; changed(); }), mid, right));
    });

    const inp = h("input", { id: "newsrc", placeholder: "Hugging Face user or organization, e.g. bartowski", "aria-label": "New source" }) as HTMLInputElement;
    const add = () => {
      const v = inp.value.trim();
      if (!v) err = "Enter a user or organization name.";
      else if (!NAME_RE.test(v)) err = "Names use letters, numbers, dots, dashes and underscores.";
      else if (S.sources.some((s) => s.name.toLowerCase() === v.toLowerCase())) err = `${v} is already in the list.`;
      else {
        S.sources.push({ id: v, name: v, builtin: false, desc: "Added by you", on: true, comps: { official: true, finetune: true, unc: false } });
        persist();
        err = "";
        toast(`${v} added`);
      }
      const keep = err ? v : "";
      draw();
      const ni = $<HTMLInputElement>("#newsrc");
      if (ni) { ni.value = keep; ni.focus(); }
    };
    inp.addEventListener("keydown", (e) => { if (e.key === "Enter") add(); });
    inp.addEventListener("input", () => { if (err) { err = ""; const el = $("#srcErr"); if (el) el.textContent = ""; } });

    const dlg = h("div", { class: "dialog", role: "dialog", "aria-modal": "true", "aria-labelledby": "srcTitle" },
      h("div", { style: "display:flex;justify-content:space-between;align-items:flex-start;gap:12px" },
        h("div", {}, h("h2", { id: "srcTitle" }, "Sources on huggingface.co"), h("p", {}, "Hugging Face accounts the app takes models from. A source that is on shows all its models. Higher sources win when two offer the same model.")),
        h("button", { class: "ghost", "aria-label": "Close", onclick: close }, "Done")),
      list,
      h("div", { class: "addrow" }, inp, h("button", { onclick: add }, "Add source")),
      h("p", { class: "err", id: "srcErr", "aria-live": "polite" }, err),
      h("p", {}, "Share a setup: ",
        h("button", { class: "linkbtn", onclick: exportSources }, "Export"), " · ",
        h("button", { class: "linkbtn", onclick: () => importSources(draw) }, "Import")));
    const scrim = h("div", { class: "scrim", onclick: (e: Event) => { if (e.target === scrim) close(); } }, dlg);
    root.replaceChildren(scrim);
  };

  draw();
  setTimeout(() => $("#newsrc")?.focus(), 0);
}
