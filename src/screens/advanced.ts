// Advanced setup: system-settings style pages, multi-GPU split and the matching command line.

import { $, h, select, toggle, toast, type Kid } from "../dom";
import { sampleConfig } from "../data";
import { isActive, launch } from "../server";
import { S, render } from "../state";

const ICONS: Record<string, string> = {
  model: "M12 3l8 4.5v9L12 21l-8-4.5v-9z M12 12l8-4.5 M12 12v9 M12 12L4 7.5",
  hardware: "M7 7h10v10H7z M9 3v4 M15 3v4 M9 17v4 M15 17v4 M3 9h4 M3 15h4 M17 9h4 M17 15h4",
  context: "M4 6h16 M4 12h16 M4 18h10",
  images: "M4 5h16v14H4z M4 16l5-5 4 4 3-3 4 4",
  voice: "M12 3a3 3 0 013 3v6a3 3 0 01-6 0V6a3 3 0 013-3z M5 11a7 7 0 0014 0 M12 18v3",
  network: "M12 3v6 M5 15v-3h14v3 M3 15h4v6H3z M10 15h4v6h-4z M17 15h4v6h-4z",
  presets: "M6 3h12v18l-6-4-6 4z",
};
const PAGES: [string, string][] = [
  ["model", "Model"], ["hardware", "Hardware"], ["context", "Context and memory"], ["images", "Images"],
  ["voice", "Voice and audio"], ["network", "Network"], ["presets", "Presets"],
];

function icon(k: string) {
  const ns = "http://www.w3.org/2000/svg";
  const s = document.createElementNS(ns, "svg");
  for (const [a, v] of Object.entries({ viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", "stroke-width": "1.8", "stroke-linecap": "round", "stroke-linejoin": "round", "aria-hidden": "true" }))
    s.setAttribute(a, v);
  const p = document.createElementNS(ns, "path");
  p.setAttribute("d", ICONS[k]);
  s.append(p);
  return s;
}

const row = (label: Kid, ctrl: Kid, hint?: string) => h("div", { class: "r" }, h("div", {}, label, hint ? h("small", {}, hint) : null), ctrl);
const muted = (t: string) => h("span", { style: "color:var(--fg-2)" }, t);

export function renderAdvanced() {
  const A = S.adv;
  const root = h("div", { class: "adv" });
  const nav = h("nav", { class: "snav", "aria-label": "Settings" },
    ...PAGES.map(([k, l]) => h("button", { "aria-current": String(A.page === k), onclick: () => { A.page = k; render(); } }, icon(k), l)));
  root.append(h("aside", { class: "side" }, nav));
  const pane = h("section", { class: "pane" });
  root.append(pane);
  const title = (t: string) => h("h1", { style: "font-size:22px" }, t);

  if (A.page === "hardware") {
    pane.append(title("Hardware"));
    const gpuSwitch = (i: 0 | 1) => toggle("gpu" + i, A.gpus[i], (v) => {
      if (!v && !A.gpus[1 - i]) { toast("Keep at least one GPU"); return render(); }
      A.gpus[i] = v;
      render();
    });
    pane.append(h("div", { class: "grp" },
      row("Backend", select("backend", A.backend, [["cuda", "CUDA (NVIDIA)"], ["vulkan", "Vulkan (any GPU)"], ["rocm", "ROCm (AMD)"], ["cpu", "CPU only"]], (v) => { A.backend = v; render(); })),
      row("Use RTX 4090 · 24 GB", gpuSwitch(0)),
      row("Use RTX 3060 · 12 GB", gpuSwitch(1)),
      row("Layers on GPU", h("span", { class: "mono", style: "font-size:13px;color:var(--fg-2)" }, `${A.layers} of 65 · auto`), "Calculated from free graphics memory")));

    const both = A.gpus[0] && A.gpus[1] && A.backend !== "cpu";
    if (both) {
      const l0 = Math.round((65 * A.split) / 100), l1 = 65 - l0;
      const model = 16.5, ctxGb = (A.ctx / 16384) * 2.2;
      const u0 = (model * A.split) / 100 + (ctxGb * A.split) / 100 + (A.main === 0 ? 0.8 : 0);
      const u1 = (model * (100 - A.split)) / 100 + (ctxGb * (100 - A.split)) / 100 + (A.main === 1 ? 0.8 : 0);
      const range = h("input", { type: "range", id: "split", min: "10", max: "90", step: "5", value: A.split, "aria-label": "Share of layers on GPU 0" }) as HTMLInputElement;
      range.addEventListener("input", () => { A.split = +range.value; render(); $("#split")?.focus(); });
      const gpuBar = (i: number, n: string, cap: number, u: number) =>
        h("div", {}, `${n} · ${cap} GB`,
          h("div", { class: "bar-track", style: "margin:5px 0" },
            h("div", { class: "bar-fill", style: `width:${Math.min(100, (u / cap) * 100)}%;background:${u > cap ? "var(--bad)" : i ? "#1d9e75" : "var(--accent)"}` })),
          h("small", { style: u > cap ? "color:var(--bad)" : "" }, `${u.toFixed(1)} GB used${u > cap ? " · too full, move the divider" : ""}${A.main === i ? " · main GPU" : ""}`));
      pane.append(
        h("p", { class: "glabel" }, "Layer split"),
        h("div", { class: "grp", style: "padding:14px;display:grid;gap:12px" },
          h("div", { class: "split" },
            h("div", { class: "g0", style: `width:${A.split}%` }, `GPU 0 · ${l0} layers`),
            h("div", { class: "g1", style: `width:${100 - A.split}%` }, `GPU 1 · ${l1} layers`)),
          range,
          h("div", { class: "gpus" }, gpuBar(0, "RTX 4090", 24, u0), gpuBar(1, "RTX 3060", 12, u1))),
        h("div", { class: "grp" },
          row("Split mode", select("mode", A.mode, [["layer", "By layer"], ["row", "By row"]], (v) => { A.mode = v; render(); }), "By row can be faster with two similar GPUs"),
          row("Main GPU", select("main", A.main, [[0, "GPU 0 · RTX 4090"], [1, "GPU 1 · RTX 3060"]], (v) => { A.main = +v; render(); })),
          row("Image model runs on", select("img", A.img, [[0, "GPU 0"], [1, "GPU 1"], [-1, "CPU"]], (v) => { A.img = +v; render(); }))));
    } else {
      pane.append(h("div", { class: "grp" },
        row("Single GPU", h("span", { style: "font-size:13px;color:var(--fg-2)" }, A.backend === "cpu" ? "GPU not used" : A.gpus[0] ? "RTX 4090" : "RTX 3060"), "Turn on both GPUs to split the model")));
    }
    pane.append(cliBox());
  } else if (A.page === "context") {
    pane.append(title("Context and memory"),
      h("div", { class: "grp" },
        row("Context size", select("ctx", A.ctx, [[4096, "4,096"], [8192, "8,192"], [16384, "16,384"], [32768, "32,768"], [65536, "65,536"]], (v) => { A.ctx = +v; render(); }), "How much of the chat the model can remember"),
        row("Flash attention", toggle("fa", A.fa, (v) => { A.fa = v; render(); }), "Less memory, usually faster"),
        row("Cache precision", select("kv", A.kv, [["f16", "Full (f16)"], ["q8", "Half (8-bit)"], ["q4", "Quarter (4-bit)"]], (v) => { A.kv = v; render(); }), "Lower saves memory, costs a little quality")),
      cliBox());
  } else {
    const noop = () => {};
    const load = (msg: string) => h("button", { onclick: () => toast(msg) }, "Load");
    const pages: Record<string, [string, [string, Kid][]]> = {
      model: ["Model", [
        ["Text model", h("span", { class: "mono", style: "font-size:12.5px" }, "Qwen3.8-27B-Q4_K_M.gguf")],
        ["Vision file", muted("Automatic")], ["Chat template", muted("Detect automatically")], ["Thinking", toggle("think", true, noop)]]],
      images: ["Images", [["Image model", muted("FLUX.2 Klein 4B")], ["Tiled decoding", toggle("tile", true, noop)]]],
      voice: ["Voice and audio", [["Voice input", muted("Whisper base.en")], ["Voice output", muted("Kokoro")]]],
      network: ["Network", [
        ["Port", h("span", { class: "mono" }, "5001")],
        ["Allow other devices on this network", toggle("lan", false, noop)], ["Remote access tunnel", toggle("tunnel", false, noop)]]],
      presets: ["Presets", [
        ["Roleplay · Qwen3.8 27B", load("Preset loaded")], ["Coding · two GPUs", load("Preset loaded")], ["MidSpec-Chatbot.kcppt", load("Template loaded")]]],
    };
    const [t, rows] = pages[A.page];
    pane.append(title(t), h("div", { class: "grp" }, ...rows.map(([l, c]) => row(l, c))), h("p", { class: "note" }, "Placeholder page, not connected yet."));
  }

  pane.append(h("div", { class: "btns" },
    h("button", { onclick: () => toast("Saved as preset") }, "Save as preset"),
    h("button", { class: "primary", onclick: () => launch("Qwen3.8 27B", sampleConfig("Qwen3.8-27B-Q4_K_M")) },
      isActive() ? "Restart server" : "Start server")));
  return root;
}

/** Command line preview. Will be generated by the Rust code that also writes .kcpps. */
function cliBox() {
  const A = S.adv;
  const parts = ["--model Qwen3.8-27B-Q4_K_M.gguf"];
  const both = A.gpus[0] && A.gpus[1];
  if (A.backend === "cuda") parts.push(`--usecuda normal ${both ? A.main : A.gpus[0] ? 0 : 1} mmq${both && A.mode === "row" ? " rowsplit" : ""}`);
  else if (A.backend === "vulkan") parts.push(`--usevulkan ${both ? "0 1" : A.gpus[0] ? "0" : "1"}`);
  else if (A.backend === "rocm") parts.push(`--usehipblas normal ${both ? A.main : 0}`);
  else parts.push("--usecpu");
  if (A.backend !== "cpu") {
    parts.push(`--gpulayers ${A.layers}`);
    if (both) parts.push(`--tensor_split ${A.split} ${100 - A.split}`);
  }
  parts.push(`--contextsize ${A.ctx}`);
  if (A.fa) parts.push("--flashattention");
  if (A.kv !== "f16") parts.push(`--quantkv ${A.kv === "q8" ? 1 : 2}`);
  const txt = "koboldcpp " + parts.join(" ");
  const code = h("code", {}, txt);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(txt);
      toast("Command copied");
    } catch {
      const r = document.createRange();
      r.selectNodeContents(code);
      getSelection()?.removeAllRanges();
      getSelection()?.addRange(r);
      toast("Selected, press Cmd+C to copy");
    }
  };
  return h("div", { style: "display:grid;gap:6px" },
    h("p", { class: "glabel", style: "margin:0 0 0 4px" }, "Same setup as a command"),
    h("div", { class: "cli" }, code, h("button", { onclick: copy }, "Copy")));
}
