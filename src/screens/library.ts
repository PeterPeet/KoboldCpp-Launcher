// Model library: local models as cards with filters, plus a Hugging Face discover tab.

import { DRIVES, LOCAL, type DriveId } from "../data";
import { $, checkbox, gb, h, toast, type Kid } from "../dom";
import { openSources } from "../dialogs/sources";
import { goControl, popular, startDownload } from "../fit";
import { launch, stop } from "../server";
import { sampleConfig } from "../data";
import { S, render } from "../state";

export function renderLibrary() {
  const L = S.lib;
  const root = h("div", { class: "lib" });
  const side = h("aside", { class: "side" });

  const q = h("input", { type: "search", id: "libq", placeholder: "Search models", value: L.q, "aria-label": "Search models" }) as HTMLInputElement;
  q.addEventListener("input", () => { L.q = q.value; renderLibGrid(); });
  side.append(q);

  const facet = (title: string, key: "types" | "fams", items: [string, string, number, Kid?][]) => {
    const box = h("div", { style: "display:grid;gap:2px" }, h("h3", { style: "margin-bottom:6px" }, title));
    for (const [val, label, n, badge] of items) {
      const id = `f-${key}-${val}`;
      box.append(h("label", { class: "chk", for: id }, badge, label, h("span", { class: "n" }, n),
        checkbox(id, L[key].has(val), () => { L[key].has(val) ? L[key].delete(val) : L[key].add(val); renderLibGrid(); })));
    }
    return box;
  };
  const count = (k: "type" | "famName", v: string) => LOCAL.filter((m) => m[k] === v).length;
  side.append(facet("Type", "types", ["Text", "Vision", "Image", "Speech", "Embedding"].map((t) => [t, t, count("type", t)])));
  const fams = [...new Map(LOCAL.map((m) => [m.famName, m.fam]))];
  side.append(facet("Family", "fams", fams.map(([n, f]) => [n, n, count("famName", n), h("span", { class: `mono-badge fam-${f}` }, n.slice(0, 2))])));

  const dbox = h("div", { style: "display:grid;gap:6px" }, h("h3", {}, "Drive"));
  for (const [k, d] of Object.entries(DRIVES)) {
    const used = LOCAL.filter((m) => m.drive === k).reduce((a, m) => a + m.size, 0);
    const pct = 100 - (d.free / d.total) * 100;
    const id = "f-drive-" + k;
    dbox.append(h("label", { class: "drive", for: id },
      h("span", { class: "chk", style: "padding:0" }, d.label, h("span", { class: "n" }),
        checkbox(id, L.drives.has(k), () => { L.drives.has(k) ? L.drives.delete(k) : L.drives.add(k); renderLibGrid(); })),
      h("div", { class: "bar-track" }, h("div", { class: "bar-fill", style: `width:${pct}%;background:${pct > 75 ? "var(--warn)" : "var(--fg-2)"}` })),
      h("small", {}, `${gb(used)} of models · ${d.free} GB free`)));
  }
  side.append(dbox);

  const main = h("section", { class: "libmain" });
  const tabs = h("div", { class: "tabs", role: "tablist" },
    ...([["local", "On this computer"], ["discover", "Discover on Hugging Face"]] as const).map(([k, l]) =>
      h("button", { role: "tab", "aria-selected": String(L.tab === k), onclick: () => { L.tab = k; render(); } }, l)));
  main.append(h("div", { style: "display:flex;justify-content:space-between;align-items:flex-end;gap:10px;flex-wrap:wrap" },
    tabs, h("button", { onclick: () => toast("Pick a folder with .gguf files to add it") }, "Add folder")));
  main.append(h("div", { id: "libBody" }));
  root.append(side, main);
  setTimeout(renderLibGrid);
  return root;
}

function renderLibGrid() {
  const L = S.lib;
  const body = $("#libBody");
  if (!body) return;
  const qq = L.q.toLowerCase();

  if (L.tab === "discover") {
    const items = popular().items.filter(({ m }) => !qq || m.name.toLowerCase().includes(qq) || m.repo.toLowerCase().includes(qq));
    const list = h("div", { class: "list" });
    for (const { m, p } of items) {
      const key = "m:" + m.repo;
      list.append(h("div", { class: "row" },
        h("div", { class: `mono-badge fam-${m.fam}` }, m.name.slice(0, 2)),
        h("div", { class: "main" },
          h("div", { class: "name" }, m.name, h("span", { class: "pill p-mute" }, p.q)),
          h("div", { class: "sub" }, `${gb(p.size)} · ${m.tags}`),
          h("div", { class: "repo" }, m.repo),
          S.downloads[key] ? goControl(key, m.name, p.size, false) : null),
        S.downloads[key] ? null : h("button", { onclick: () => startDownload(key, m.name, p.size) }, "Download")));
    }
    body.replaceChildren(h("div", { style: "display:grid;gap:10px" },
      h("p", { class: "note", style: "margin:0" },
        `Searching ${S.sources.filter((s) => s.on && s.comps).map((s) => s.name).join(", ")} · `,
        h("button", { class: "linkbtn", onclick: openSources }, "Manage sources")),
      items.length ? list : h("div", { class: "empty" }, "No results from your sources.")));
    return;
  }

  const items = LOCAL.filter((m) =>
    (!L.types.size || L.types.has(m.type)) &&
    (!L.fams.size || L.fams.has(m.famName)) &&
    (!L.drives.size || L.drives.has(m.drive)) &&
    (!qq || (m.name + m.tags).toLowerCase().includes(qq)));
  const total = items.reduce((a, m) => a + m.size, 0);
  const grid = h("div", { class: "grid" });
  const kv = (k: string, v: Kid, style?: string) => h("div", { class: "kv" }, h("span", {}, k), h("span", { style }, v));

  for (const m of items) {
    const fit = m.fit === "ok" ? h("span", { class: "pill p-ok" }, "Fits") : h("span", { class: "pill p-warn" }, "Partly on CPU");
    const acts = h("div", { class: "acts" });
    const other: DriveId = m.drive === "C" ? "E" : "C";
    if (L.confirm === m.id)
      acts.append(h("div", { class: "confirm" }, `Delete ${gb(m.size)} from ${m.drive}:?`,
        h("button", { onclick: () => { LOCAL.splice(LOCAL.indexOf(m), 1); L.confirm = null; render(); toast(`${m.name} removed`); } }, "Delete"),
        h("button", { class: "ghost", onclick: () => { L.confirm = null; renderLibGrid(); } }, "Keep")));
    else
      acts.append(
        h("button", {
          class: S.running === m.name ? "" : "primary",
          onclick: () => (S.running === m.name ? stop() : launch(m.name, sampleConfig(m.name))),
        }, S.running === m.name ? "Stop" : "Start"),
        h("button", { onclick: () => { m.drive = other; render(); toast(`${m.name} moved to ${DRIVES[other].label}`); } }, `Move to ${other}:`),
        h("button", { class: "ghost", onclick: () => { L.confirm = m.id; renderLibGrid(); } }, "Remove"));
    grid.append(h("article", { class: "card" },
      h("div", { class: "top" }, h("span", { class: `mono-badge fam-${m.fam}`, style: "width:30px;height:30px;font-size:12px" }, m.famName.slice(0, 2)), fit),
      h("h4", {}, m.name),
      h("div", { class: "tags" }, m.tags),
      kv("Type", m.type), kv("Size", gb(m.size)), kv("Context", m.ctx),
      kv("Speed here", m.speed, m.speed.startsWith("est") ? "color:var(--fg-3)" : undefined),
      kv("Drive", DRIVES[m.drive].label), kv("Last used", m.used),
      acts));
  }
  body.replaceChildren(h("div", { style: "display:grid;gap:10px" },
    h("div", { class: "note", style: "margin:0" }, `${items.length} models · ${gb(total)}`),
    items.length ? grid : h("div", { class: "empty" }, "No models match these filters.")));
}
