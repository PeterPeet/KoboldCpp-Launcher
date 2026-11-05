// Server banner shown on the start screen: running / starting / failed, with log drawer.

import { api } from "../api";
import { reveal } from "../dialogs/update";
import { h } from "../dom";
import { logView, openLite, serverPort, stop } from "../server";
import { S, render } from "../state";

let showLog = false;

export function serverBanner(): HTMLElement[] {
  const s = S.server;
  if (s.state === "stopped" && !S.running) return [];
  const logBtn = h("button", { class: "ghost", onclick: () => { showLog = !showLog; render(); } }, showLog ? "Hide log" : "Show log");
  const out: HTMLElement[] = [];
  if (s.state === "running")
    out.push(h("div", { class: "running" },
      h("span", { class: "dot", style: "background:var(--ok)" }),
      h("div", { class: "main" }, `${S.running ?? "KoboldCpp"} is running on localhost:${serverPort()}`),
      h("button", { onclick: openLite }, "Open Lite"), logBtn,
      h("button", { class: "ghost", onclick: stop }, "Stop")));
  else if (s.state === "starting")
    out.push(h("div", { class: "running busy" },
      h("span", { class: "spinner", "aria-hidden": "true" }),
      h("div", { class: "main" }, `Starting ${S.running ?? "KoboldCpp"}… Models from templates download first, this can take a while.`),
      logBtn, h("button", { class: "ghost", onclick: stop }, "Cancel")));
  else if (s.state === "failed")
    out.push(h("div", { class: "running bad" },
      h("span", { class: "dot", style: "background:var(--bad)" }),
      h("div", { class: "main" }, s.message),
      logBtn,
      h("button", { class: "ghost", onclick: async () => reveal(`${(await api.paths()).logs}/koboldcpp.log`) }, "Open log file"),
      h("button", { class: "ghost", onclick: () => { S.server = { state: "stopped" }; S.running = null; render(); } }, "Dismiss")));
  if (showLog) out.push(logView());
  return out;
}
