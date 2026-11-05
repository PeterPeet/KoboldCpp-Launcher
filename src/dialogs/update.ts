// KoboldCpp install/update: top-bar button and dialog, backed by GitHub releases.

import { api, fmtBytes, inTauri, on, type Progress, type SettingsPatch } from "../api";
import { $, h, toggle, toast } from "../dom";
import { S, render } from "../state";

export const REPO = "https://github.com/LostRuins/koboldcpp";

const installed = () => S.binary?.installed && S.binary.present;

export function drawUpdBtn() {
  const b = $("#updBtn")!;
  const ver = S.binary?.installed?.version;
  $("#verLabel")!.textContent = installed() ? ver ?? "custom build" : "not installed";
  if (!installed()) {
    b.className = "upd new";
    b.textContent = S.binary?.asset ? "Install KoboldCpp" : "KoboldCpp";
  } else if (S.update?.updateAvailable && S.update.latest) {
    b.className = "upd new";
    b.textContent = `KoboldCpp: Update to ${S.update.latest.tag}`;
  } else {
    b.className = "upd ok";
    b.textContent = S.update ? "KoboldCpp: Up to date" : "KoboldCpp: Check for updates";
  }
  drawSelfUpd();
}

/** This launcher's own status: it doesn't update itself, an update opens its release page. */
const LAUNCHER_REPO = "https://github.com/PeterPeet/KoboldCpp-Launcher";

function drawSelfUpd() {
  const b = $("#selfUpd");
  if (!b) return;
  const u = S.selfUpdate;
  const url = u?.updateAvailable && u.url ? u.url : `${LAUNCHER_REPO}/releases`;
  b.className = u?.updateAvailable ? "upd new" : "upd ok";
  b.textContent = !u ? "Peter's Launcher" : u.updateAvailable ? `Peter's Launcher: Update to ${u.latest}` : "Peter's Launcher: Up to date";
  b.title = u ? `This launcher: ${u.current}${u.latest ? `, newest release ${u.latest}` : ", no release published yet"}. Opens the releases page.` : "Open the launcher's releases page";
  b.onclick = () => import("@tauri-apps/plugin-opener").then((m) => m.openUrl(url)).catch(() => window.open(url, "_blank", "noopener"));
}

export async function refreshSelf() {
  try {
    S.selfUpdate = await api.launcherUpdate();
  } catch {
    /* offline: the button just shows the name */
  }
  drawSelfUpd();
}

export async function refreshBinary(check: boolean) {
  try {
    S.binary = await api.binaryStatus();
    if (check) S.update = await api.checkUpdate();
  } catch (e) {
    S.updateError = String(e);
  }
  drawUpdBtn();
}

function fmtDate(iso: string | null) {
  return iso ? new Date(iso).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" }) : "";
}

/** Show a file in Finder / Explorer. */
export async function reveal(path: string) {
  if (!inTauri) return toast(path);
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  revealItemInDir(path).catch((e) => toast(String(e)));
}

export function openUpdate() {
  const root = $("#dialogRoot")!;
  let progress: Progress | null = null;
  let working = false;
  let error = "";
  const close = () => { if (working) return; root.replaceChildren(); render(); };

  const unlisten = on<Progress>("binary-progress", (p) => {
    progress = p;
    const f = $("#updFill"), s = $("#updStep");
    const pct = p.total ? (p.done / p.total) * 100 : 0;
    if (f) f.style.width = pct + "%";
    if (s) s.textContent = `Downloading ${fmtBytes(p.done)}${p.total ? " of " + fmtBytes(p.total) : ""}${p.speed ? ` · ${fmtBytes(p.speed)}/s` : ""}`;
  });

  const run = async () => {
    working = true;
    error = "";
    draw();
    try {
      const b = await api.installBinary(S.update?.latest?.tag);
      await refreshBinary(false);
      if (S.update) S.update = { ...S.update, current: b.version, updateAvailable: false };
      working = false;
      unlisten();
      root.replaceChildren();
      drawUpdBtn();
      render();
      toast(`KoboldCpp ${b.version ?? ""} installed` + (S.running ? `. ${S.running} is running again.` : ""));
    } catch (e) {
      working = false;
      error = String(e);
      draw();
    }
  };

  const check = async () => {
    error = "";
    try {
      S.update = await api.checkUpdate();
      drawUpdBtn();
      if (!S.update.updateAvailable) toast("Checked GitHub: no newer release");
    } catch (e) {
      error = String(e);
    }
    draw();
  };

  const saveSetting = async (patch: SettingsPatch) => {
    if (!S.settings) return;
    S.settings = await api.saveSettings(patch);
  };

  function draw() {
    const U = S.update;
    const has = !installed() || !!U?.updateAvailable;
    const latest = U?.latest;
    // Links from GitHub's answer: only https (no javascript: or file: links).
    const link = (href: string, text: string, style = "color:var(--accent-text)") =>
      /^https:\/\//i.test(href) ? h("a", { href, target: "_blank", rel: "noopener", style }, text) : h("span", {}, text);
    const title = !installed()
      ? "Install KoboldCpp"
      : has && latest ? `KoboldCpp ${latest.tag} is available` : "You're up to date";
    const sub = !installed()
      ? S.binary?.asset
        ? `The launcher downloads KoboldCpp ${latest?.tag ?? ""} (${S.binary.asset}${latest?.size ? ", " + fmtBytes(latest.size) : ""}) from GitHub and checks it.`
        : "KoboldCpp has no official build for this computer. Pick a koboldcpp file you built yourself."
      : has && latest
        ? `You have ${S.binary?.installed?.version ?? "a custom build"}. Released on GitHub on ${fmtDate(latest.publishedAt)}.`
        : `KoboldCpp ${S.binary?.installed?.version ?? ""} is the newest release.`;

    const body = h("div", { class: "dialog", role: "dialog", "aria-modal": "true", "aria-labelledby": "updTitle", style: "width:min(600px,100%)", "data-busy": working ? "1" : "" });
    body.append(h("div", { style: "display:flex;justify-content:space-between;align-items:flex-start;gap:12px" },
      h("div", {}, h("h2", { id: "updTitle" }, title), h("p", {}, sub)),
      working ? null : h("button", { class: "ghost", "aria-label": "Close", onclick: close }, "Close")));

    if (has && latest?.notes)
      body.append(h("div", { class: "notes" }, h("strong", {}, "What's new"),
        h("pre", { class: "relnotes" }, latest.notes.split("\n").slice(0, 40).join("\n")),
        latest.url ? h("div", { style: "margin-top:6px" }, link(latest.url, "Read the full notes on GitHub")) : null));

    if (working) {
      const pct = progress?.total ? (progress.done / progress.total) * 100 : 0;
      body.append(h("div", { class: "prog" },
        h("div", { class: "bar-track" }, h("div", { class: "bar-fill", id: "updFill", style: `width:${pct}%` })),
        h("small", { id: "updStep" }, "Starting download")));
    } else if (has && installed()) {
      body.append(h("p", {}, S.running
        ? `${S.running} stops while updating and starts again afterwards. Your models, presets and sources stay as they are.`
        : "Your models, presets and sources stay as they are."));
    }
    const path = S.binary?.installed?.path;
    if (installed() && path)
      body.append(h("div", { class: "notes", style: "display:flex;gap:10px;align-items:center" },
        h("div", { style: "flex:1;min-width:0" }, h("strong", {}, "Installed at"), h("div", { class: "mono", style: "font-size:12px;overflow-wrap:anywhere" }, path)),
        h("button", { onclick: () => reveal(path) }, "Show in Finder")));
    if (error || S.updateError) body.append(h("p", { class: "err" }, error || S.updateError));

    if (S.settings)
      body.append(h("div", { style: "display:grid;gap:8px;border-top:1px solid var(--line);padding-top:12px" },
        toggle("updStart", S.settings.checkUpdatesOnStart, (v) => saveSetting({ checkUpdatesOnStart: v }), "Check for updates when the app starts"),
        toggle("updPre", S.settings.includePrereleases, async (v) => { await saveSetting({ includePrereleases: v }); await check(); }, "Include pre-releases")));

    const btns = h("div", { class: "btns" });
    if (!working) {
      if (has && S.binary?.asset && latest)
        btns.append(link(`${REPO}/releases`, "View on GitHub", "align-self:center;font-size:13px;color:var(--accent-text);margin-right:auto"),
          ...(installed() ? [h("button", { onclick: close }, "Later")] : []),
          h("button", { class: "primary", onclick: run }, installed() ? "Update now" : "Download and install"));
      else
        btns.append(h("button", { onclick: check }, "Check now"), h("button", { class: "primary", onclick: close }, "Done"));
    } else {
      btns.append(h("button", { onclick: () => api.cancelInstall() }, "Cancel"));
    }
    body.append(btns);
    const scrim = h("div", { class: "scrim", onclick: (e: Event) => { if (e.target === scrim) close(); } }, body);
    root.replaceChildren(scrim);
  }

  draw();
  if (!S.update) check();
}
