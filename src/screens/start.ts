// Start screen: hardware choice, KoboldCpp templates, popular models.

import { TEMPLATES, TIERS, type TierId } from "../data";
import { checkbox, fmt, gb, h, toggle, toast } from "../dom";
import { openSources } from "../dialogs/sources";
import { fitPill, goControl, popular, speed, stopServer, tplFit } from "../fit";
import { S, render, saveTier, vram } from "../state";

export function renderStart() {
  const t = TIERS[S.tier];
  const root = h("div", { class: "start" });

  if (S.running)
    root.append(h("div", { class: "running" },
      h("span", { class: "dot", style: "background:var(--ok)" }),
      h("div", { class: "main" }, `${S.running} is running on localhost:5001`),
      h("button", { onclick: () => toast("Opens Lite in your browser") }, "Open Lite"),
      h("button", { class: "ghost", onclick: stopServer }, "Stop")));

  const tierSel = h("select", { id: "tierSel", "aria-label": "Simulated hardware" },
    ...Object.entries(TIERS).map(([k, v]) => h("option", { value: k, selected: k === S.tier }, v.label))) as HTMLSelectElement;
  tierSel.addEventListener("change", () => {
    S.tier = tierSel.value as TierId;
    S.useGpu = TIERS[S.tier].gpus.map(() => true);
    saveTier();
    render();
  });

  root.append(h("div", { style: "display:grid;gap:8px" },
    h("h1", {}, "Pick your hardware setup to start"),
    h("div", { class: "hw" },
      h("span", {}, "Run on"),
      ...t.gpus.map((g, i) =>
        h("label", { class: "hwchip" + (S.useGpu[i] ? " on" : ""), for: "gpu-" + i },
          checkbox("gpu-" + i, S.useGpu[i], (v) => {
            S.useGpu[i] = v;
            render();
            toast(vram() ? `Using ${vram()} GB graphics memory` : "CPU only: slower, but runs without a GPU");
          }),
          `${g.name} · ${g.vram} GB`)),
      h("span", { class: "hwchip on fixed", title: "The CPU is always used" }, `CPU · ${t.cpu} · ${t.ram} GB RAM`),
      h("span", { class: "pill p-acc" }, vram() ? `${t.label} · ${vram()} GB GPU` : "CPU only"),
      h("label", { class: "note", style: "margin:0;display:inline-flex;gap:6px;align-items:center" }, "Pretend this PC is", tierSel))));

  // KoboldCpp templates
  const ks = S.sources.find((s) => s.id === "kcppt");
  if (ks?.on) {
    const list = TEMPLATES.filter((x) => S.showAllTiers || x.tier === S.tier)
      .map((x) => ({ x, f: tplFit(x) }))
      .filter((o) => S.showAllTiers || o.f !== "none");
    root.append(h("div", { class: "sec-head" },
      h("div", {},
        h("h2", {}, "KoboldCpp's ready-made setups"),
        h("p", {}, `KoboldCpp's own templates for ${t.label.toLowerCase()} PCs. One click downloads every model in it.`)),
      toggle("allTiers", S.showAllTiers, (v) => { S.showAllTiers = v; render(); }, "Show other tiers")));
    const tiles = h("div", { class: "tiles" });
    list.forEach(({ x, f }, i) =>
      tiles.append(h("div", { class: "tile" + (i === 0 && !S.showAllTiers ? " best" : "") },
        h("div", { class: "tile-head" }, h("div", {}, h("h4", {}, x.use), h("div", { class: "file" }, x.id + ".kcppt")), fitPill(f)),
        h("div", { class: "parts" }, ...x.parts.map((p) => h("span", {}, p))),
        h("div", { class: "meta" }, `${gb(x.size)} download` + (x.ctx ? ` · ${x.ctx}` : "")),
        f === "none"
          ? h("div", { class: "meta", style: "color:var(--bad)" }, "Needs more graphics memory than this PC has")
          : goControl("t:" + x.id, x.use, x.size, false))));
    root.append(list.length ? tiles : h("div", { class: "empty" }, "No templates for this tier."));
  }

  // Popular models
  const pop = popular();
  const seg = h("div", { class: "seg", role: "group", "aria-label": "Which models" },
    ...([["Official releases", true], ["All models", false]] as const).map(([l, v]) =>
      h("button", { "aria-pressed": String(S.official === v), onclick: () => { S.official = v; render(); } }, l)));
  root.append(h("div", { class: "sec-head" },
    h("div", {},
      h("h2", {}, "Choose from popular models yourself"),
      h("p", {}, "Ranked by downloads, likes and age from your sources. The best quality that fits is picked for you.")),
    h("div", { class: "controls" }, seg,
      toggle("unc", S.uncensored, (v) => { S.uncensored = v; render(); }, "Show uncensored finetunes"))));

  const list = h("div", { class: "list" });
  if (!pop.items.length) list.append(h("div", { class: "empty" }, "Nothing matches. Turn on more sources or switch to All models."));
  for (const { m, p } of pop.items) {
    const key = "m:" + m.repo;
    const badge = m.fam === "ll" && m.name.startsWith("Deep") ? "De" : m.name.slice(0, 2);
    list.append(h("div", { class: "row" },
      h("div", { class: `mono-badge fam-${m.fam}` }, badge),
      h("div", { class: "main" },
        h("div", { class: "name" }, m.name,
          m.kind === "official" ? h("span", { class: "pill p-mute" }, "Official") : h("span", { class: "pill p-acc" }, "Finetune"),
          m.unc ? h("span", { class: "pill p-warn" }, "Uncensored") : null,
          p.fit !== "full" ? fitPill(p.fit) : null),
        h("div", { class: "sub" }, `${m.params} · ${p.q} · ${gb(p.size)} · about ${speed(p)} words/s · ${m.tags}`),
        h("div", { class: "why" }, h("span", { class: "repo" }, m.repo), ` · ${fmt(m.dl)} downloads this month · ${m.likes} likes · ${m.age} days old`),
        S.downloads[key] ? goControl(key, m.name, p.size, false) : null),
      S.downloads[key] ? null : goControl(key, m.name, p.size, false)));
  }
  root.append(list);

  if (!S.official && !S.sources.some((s) => s.on && s.comps && (s.comps.finetune || s.comps.unc)))
    root.append(h("p", { class: "note", style: "margin:-10px 0 0" },
      "Your sources only allow official models, so All models shows the same list. ",
      h("button", { class: "linkbtn", onclick: openSources }, "Add bartowski or TheDrummer"), " to see finetunes."));

  const hidden = [pop.tooBig ? `${pop.tooBig} too big for this PC, hidden` : "", pop.dupes ? `${pop.dupes} duplicate upload${pop.dupes > 1 ? "s" : ""} merged` : ""]
    .filter(Boolean).join(" · ");
  root.append(h("div", { class: "foot" },
    h("span", {}, hidden || "Nothing hidden"),
    h("span", {},
      h("button", { class: "linkbtn", onclick: openSources }, "Manage sources"), " · ",
      h("a", { href: "#library", style: "color:var(--accent-text)" }, "Model library"), " · ",
      h("a", { href: "#advanced", style: "color:var(--accent-text)" }, "Advanced setup"))));
  return root;
}
