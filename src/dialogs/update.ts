// Update button and dialog (simulated; will check GitHub releases later).

import { $, h, toggle, toast } from "../dom";
import { S, render } from "../state";

export const REPO = "https://github.com/LostRuins/koboldcpp";

export function drawUpdBtn() {
  const U = S.upd;
  const b = $("#updBtn")!;
  const has = U.ver !== U.latest;
  b.className = "upd " + (has ? "new" : "ok");
  b.textContent = has ? `Update to ${U.latest}` : "Up to date";
  $("#verLabel")!.textContent = U.ver;
}

export function openUpdate() {
  const root = $("#dialogRoot")!;
  const U = S.upd;
  const close = () => { if (U.state === "working") return; root.replaceChildren(); render(); };

  const run = () => {
    const wasRunning = S.running;
    U.state = "working";
    U.pct = 0;
    U.step = "Downloading the new version";
    draw();
    const tick = () => {
      U.pct = Math.min(100, U.pct + 5 + Math.random() * 8);
      U.step = U.pct < 70 ? "Downloading the new version" : U.pct < 88 ? "Checking the download" : "Restarting KoboldCpp";
      const f = $("#updFill"), s = $("#updStep");
      if (f) f.style.width = U.pct + "%";
      if (s) s.textContent = U.step;
      if (U.pct >= 100) {
        U.ver = U.latest;
        U.state = "idle";
        drawUpdBtn();
        root.replaceChildren();
        render();
        toast(`Updated to ${U.ver}` + (wasRunning ? `. ${wasRunning} is running again.` : ""));
        return;
      }
      setTimeout(tick, 160);
    };
    setTimeout(tick, 160);
  };

  function draw() {
    const has = U.ver !== U.latest;
    const link = (href: string, text: string, style = "color:var(--accent-text)") => h("a", { href, target: "_blank", rel: "noopener", style }, text);
    const body = h("div", { class: "dialog", role: "dialog", "aria-modal": "true", "aria-labelledby": "updTitle", style: "width:min(560px,100%)" });
    body.append(h("div", { style: "display:flex;justify-content:space-between;align-items:flex-start;gap:12px" },
      h("div", {},
        h("h2", { id: "updTitle" }, has ? `KoboldCpp ${U.latest} is available` : "You're up to date"),
        h("p", {}, has ? `You have ${U.ver}. Released on GitHub on ${U.date}.` : `KoboldCpp ${U.ver} is the newest release.`)),
      U.state === "working" ? null : h("button", { class: "ghost", "aria-label": "Close", onclick: close }, "Close")));
    if (has)
      body.append(h("div", { class: "notes" }, h("strong", {}, "What's new"),
        h("ul", {}, h("li", {}, "The release notes from GitHub appear here."), h("li", {}, "Sample text for now.")),
        h("div", { style: "margin-top:6px" }, link(`${REPO}/releases/tag/${U.latest}`, "Read the full notes on GitHub"))));
    if (U.state === "working")
      body.append(h("div", { class: "prog" },
        h("div", { class: "bar-track" }, h("div", { class: "bar-fill", id: "updFill", style: `width:${U.pct}%` })),
        h("small", { id: "updStep" }, U.step)));
    else if (has)
      body.append(h("p", {}, S.running
        ? `${S.running} stops while updating and starts again afterwards. Your models, presets and sources stay as they are.`
        : "Your models, presets and sources stay as they are."));
    body.append(h("div", { style: "display:grid;gap:8px;border-top:1px solid var(--line);padding-top:12px" },
      toggle("updStart", U.onStart, (v) => { U.onStart = v; }, "Check for updates when the app starts"),
      toggle("updPre", U.pre, (v) => { U.pre = v; toast(v ? "Pre-releases included" : "Stable releases only"); }, "Include pre-releases")));
    const btns = h("div", { class: "btns" });
    if (U.state !== "working") {
      if (has)
        btns.append(link(`${REPO}/releases`, "View on GitHub", "align-self:center;font-size:13px;color:var(--accent-text);margin-right:auto"),
          h("button", { onclick: close }, "Later"),
          h("button", { class: "primary", onclick: run }, "Update now"));
      else
        btns.append(h("button", { onclick: () => toast("Checked GitHub: no newer release") }, "Check now"),
          h("button", { class: "primary", onclick: close }, "Done"));
    }
    body.append(btns);
    const scrim = h("div", { class: "scrim", onclick: (e: Event) => { if (e.target === scrim) close(); } }, body);
    root.replaceChildren(scrim);
  }

  draw();
}
