// Sources dialog: APT-style list of Hugging Face uploaders with components and priority.

import { $, checkbox, h, toggle, toast } from "../dom";
import { S, render } from "../state";

const NAME_RE = /^[A-Za-z0-9][A-Za-z0-9_.-]{1,95}$/;

export function openSources() {
  const root = $("#dialogRoot")!;
  let err = "";
  const close = () => { root.replaceChildren(); render(); };

  const draw = () => {
    const list = h("div", { style: "border:1px solid var(--line);border-radius:12px" });
    S.sources.forEach((s, i) => {
      const mid = h("div", {},
        h("div", { class: "nm" }, s.name, s.builtin ? h("span", { class: "pill p-mute", style: "margin-left:6px" }, "Built in") : null),
        h("div", { class: "ds" }, s.desc));
      if (s.comps) {
        const c = s.comps;
        mid.append(h("div", { class: "comps" },
          ...([["official", "Official"], ["finetune", "Finetunes"], ["unc", "Uncensored"]] as const).map(([k, l]) =>
            h("label", { for: `c-${s.id}-${k}` }, checkbox(`c-${s.id}-${k}`, c[k], (v) => { c[k] = v; draw(); }, { disabled: !s.on }), l))));
      }
      const swap = (j: number) => { [S.sources[i], S.sources[j]] = [S.sources[j], S.sources[i]]; draw(); };
      const ord = h("div", { class: "ord" },
        h("button", { "aria-label": `Move ${s.name} up`, disabled: i === 0, onclick: () => swap(i - 1) }, "▲"),
        h("button", { "aria-label": `Move ${s.name} down`, disabled: i === S.sources.length - 1, onclick: () => swap(i + 1) }, "▼"));
      const right = h("div", { style: "display:flex;gap:8px;align-items:center" },
        s.builtin ? null : h("button", { class: "ghost", "aria-label": `Remove ${s.name}`, onclick: () => { S.sources.splice(i, 1); draw(); } }, "Remove"),
        ord);
      list.append(h("div", { class: "src" }, toggle(`src-${s.id}`, s.on, (v) => { s.on = v; draw(); }), mid, right));
    });

    const inp = h("input", { id: "newsrc", placeholder: "Hugging Face user or organization, e.g. bartowski", "aria-label": "New source" }) as HTMLInputElement;
    const add = () => {
      const v = inp.value.trim();
      if (!v) err = "Enter a user or organization name.";
      else if (!NAME_RE.test(v)) err = "Names use letters, numbers, dots, dashes and underscores.";
      else if (S.sources.some((s) => s.name.toLowerCase() === v.toLowerCase())) err = `${v} is already in the list.`;
      else {
        S.sources.push({ id: v, name: v, desc: "Repo count appears once Hugging Face is connected", on: true, comps: { official: true, finetune: true, unc: false } });
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
        h("div", {}, h("h2", { id: "srcTitle" }, "Sources"), h("p", {}, "Where the app looks for models. Higher sources win when two offer the same model.")),
        h("button", { class: "ghost", "aria-label": "Close", onclick: close }, "Done")),
      list,
      h("div", { class: "addrow" }, inp, h("button", { onclick: add }, "Add source")),
      h("p", { class: "err", id: "srcErr", "aria-live": "polite" }, err),
      h("p", {}, "Share a setup: ",
        h("button", { class: "linkbtn", onclick: () => toast("sources.json exported") }, "Export"), " · ",
        h("button", { class: "linkbtn", onclick: () => toast("Pick a sources.json to import") }, "Import")));
    const scrim = h("div", { class: "scrim", onclick: (e: Event) => { if (e.target === scrim) close(); } }, dlg);
    root.replaceChildren(scrim);
  };

  draw();
  setTimeout(() => $("#newsrc")?.focus(), 0);
}
