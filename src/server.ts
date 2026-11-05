// KoboldCpp process state in the UI: status pill, start/stop, log buffer.

import { api, on, type Kcpps, type ServerStatus } from "./api";
import { h, toast } from "./dom";
import { S, render } from "./state";

export function statusText(s: ServerStatus, label: string | null) {
  const what = label ? ` · ${label}` : "";
  switch (s.state) {
    case "starting": return `Starting${what}`;
    case "running": return `Running${what}`;
    case "failed": return "Failed to start";
    default: return "Server stopped";
  }
}

export function setStatus() {
  const st = document.getElementById("status")!;
  st.classList.toggle("on", S.server.state === "running");
  st.classList.toggle("busy", S.server.state === "starting");
  st.classList.toggle("bad", S.server.state === "failed");
  st.title = S.server.state === "failed" ? S.server.message : statusText(S.server, S.running);
  document.getElementById("statusText")!.textContent = statusText(S.server, S.running);
  // Green "Open Lite" next to the status while a model is running.
  const lite = document.getElementById("liteBtn");
  if (lite) {
    lite.hidden = S.server.state !== "running";
    lite.onclick = openLite;
  }
}

export const isActive = () => S.server.state === "starting" || S.server.state === "running";
export const serverPort = () => (S.server.state === "running" || S.server.state === "starting" ? S.server.port : 5001);

/** Start KoboldCpp with a config; `label` is what the UI shows (model or template name). */
export async function launch(label: string, config: Kcpps) {
  S.running = label;
  S.logs = [];
  try {
    const port = await api.startServer(config);
    S.server = { state: "starting", port };
  } catch (e) {
    S.running = null;
    const msg = String(e);
    if (msg.includes("not installed")) {
      toast("Install KoboldCpp first. It only takes a minute.");
      import("./dialogs/update").then((m) => m.openUpdate());
    } else {
      toast(msg);
    }
  }
  setStatus();
  render();
}

export async function stop() {
  try {
    await api.stopServer();
  } catch (e) {
    toast(String(e));
  }
  S.server = { state: "stopped" };
  S.running = null;
  setStatus();
  render();
}

export function openLite() {
  const url = `http://localhost:${serverPort()}`;
  import("@tauri-apps/plugin-opener").then((m) => m.openUrl(url)).catch(() => window.open(url, "_blank", "noopener"));
}

/** Log drawer content (last lines of KoboldCpp's output). */
export function logView() {
  const pre = h("pre", { class: "log", id: "logView" }, S.logs.slice(-300).join("\n") || "No output yet.");
  setTimeout(() => (pre.scrollTop = pre.scrollHeight));
  return pre;
}

/** While a model runs, ask KoboldCpp for the speed of the last answer; Rust stores it per model. */
function pollSpeed() {
  setInterval(() => {
    if (S.server.state === "running") api.serverPerf().catch(() => {});
  }, 15000);
}

export function initServer() {
  pollSpeed();
  api.serverStatus().then((s) => { S.server = s; setStatus(); }).catch(() => {});
  on<{ type: string; status?: ServerStatus; line?: string }>("server", (ev) => {
    if (ev.type === "log" && ev.line != null) {
      S.logs.push(ev.line);
      if (S.logs.length > 2000) S.logs.splice(0, S.logs.length - 2000);
      const v = document.getElementById("logView");
      if (v) {
        const atEnd = v.scrollTop + v.clientHeight >= v.scrollHeight - 20;
        v.textContent = S.logs.slice(-300).join("\n");
        if (atEnd) v.scrollTop = v.scrollHeight;
      }
      return;
    }
    if (ev.status) {
      const was = S.server.state;
      S.server = ev.status;
      if (ev.status.state === "running" && was !== "running") toast(`${S.running ?? "KoboldCpp"} is running on localhost:${ev.status.port}`);
      if (ev.status.state === "failed") toast(ev.status.message.slice(0, 200));
      if (ev.status.state === "stopped" || ev.status.state === "failed") S.running = ev.status.state === "failed" ? S.running : null;
      setStatus();
      render();
    }
  });
}
