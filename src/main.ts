import "@fontsource/figtree/400.css";
import "@fontsource/figtree/500.css";
import "@fontsource/figtree/600.css";
import "@fontsource/figtree/700.css";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/500.css";
import "./styles.css";
import { applyTheme } from "./theme";
applyTheme();

import { openUrl } from "@tauri-apps/plugin-opener";
import { $, toast } from "./dom";
import { openSources } from "./dialogs/sources";
import { api, fixtureMode, inTauri } from "./api";
import { applyFlavor, progName } from "./flavor";
import { drawUpdBtn, openUpdate, refreshBinary, refreshSelf } from "./dialogs/update";
import { initServer, setStatus } from "./server";
import { drawIndicator, initDownloads } from "./downloads";
import { renderAdvanced } from "./screens/advanced";
import { invalidateLibrary, renderLibrary } from "./screens/library";
import { renderStart } from "./screens/start";
import { S, setRender } from "./state";

type Route = "start" | "library" | "advanced";

function route(): Route {
  const r = (location.hash || "#start").slice(1);
  return r === "library" || r === "advanced" ? r : "start";
}

function render() {
  const r = route();
  document.querySelectorAll<HTMLElement>("[data-route]").forEach((a) => a.setAttribute("aria-current", a.dataset.route === r ? "page" : "false"));
  const scr = $("#screen")!;
  // Keep the text field the user is typing in (focus and cursor) across a redraw. Fields are
  // matched by id, else by placeholder (e.g. the Discover search box has no id).
  const act = document.activeElement as HTMLInputElement | null;
  const typing = act && (act.tagName === "INPUT" || act.tagName === "TEXTAREA") && scr.contains(act);
  const key = typing ? act!.id || act!.placeholder : "";
  const sel = typing ? [act!.selectionStart, act!.selectionEnd] : null;
  const focusId = act?.id;
  scr.replaceChildren(r === "library" ? renderLibrary() : r === "advanced" ? renderAdvanced() : renderStart());
  if (key) {
    const again = [...scr.querySelectorAll<HTMLInputElement>("input, textarea")].find((i) => (i.id || i.placeholder) === key);
    if (again) {
      again.focus();
      try {
        if (sel && sel[0] != null) again.setSelectionRange(sel[0], sel[1]);
      } catch {
        /* inputs like number fields have no selection */
      }
    }
  } else if (focusId && focusId !== "libq") document.getElementById(focusId)?.focus();
  setStatus();
  drawIndicator();
}
setRender(render);

// External links open in the system browser (inside Tauri), or a new tab in a plain browser.
document.addEventListener("click", (e) => {
  const a = (e.target as Element).closest<HTMLAnchorElement>('a[target="_blank"]');
  if (!a) return;
  e.preventDefault();
  openUrl(a.href).catch(() => window.open(a.href, "_blank", "noopener"));
});

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && $("#dialogRoot")!.firstChild && !$('#dialogRoot [data-busy="1"]')) {
    $("#dialogRoot")!.replaceChildren();
    render();
  }
});

$("#openSources")!.addEventListener("click", openSources);
$("#updBtn")!.addEventListener("click", openUpdate);
if (!inTauri && !fixtureMode) $("#previewPill")!.hidden = false;
// Screenshots: `?fixture&theme=lite` shows the KoboldAI Lite colors without storing the choice.
const shotTheme = fixtureMode ? new URLSearchParams(location.search).get("theme") : null;
if (shotTheme === "lite" || shotTheme === "light") applyTheme(shotTheme);
drawUpdBtn();
initServer();
initDownloads();

async function init() {
  try {
    S.settings = await api.settings();
    S.sources = S.settings.sources;
  } catch {
    /* defaults */
  }
  applyFlavor();
  api.hardware().then((hw) => {
    S.hw = hw;
    S.useGpu = hw.gpus.map(() => true);
    render();
  }, (e) => toast(`Hardware detection failed: ${e}`));
  if (S.settings?.checkUpdatesOnStart) refreshSelf();
  await refreshBinary(!!S.settings?.checkUpdatesOnStart);
  if (S.binary && !S.binary.present) toast(`${progName()} is not installed yet. Use the button at the top to install it.`);
  else if (S.update?.updateAvailable && S.update.latest) toast(`${progName()} ${S.update.latest.tag} is available. Use the button at the top to update.`);
  render();
}
init();

addEventListener("hashchange", () => {
  // Fresh scan whenever the library is opened (downloads, moves, files added outside).
  if (route() === "library") invalidateLibrary();
  render();
});
render();
