import "@fontsource/figtree/400.css";
import "@fontsource/figtree/500.css";
import "@fontsource/figtree/600.css";
import "@fontsource/figtree/700.css";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/500.css";
import "./styles.css";

import { openUrl } from "@tauri-apps/plugin-opener";
import { $, toast } from "./dom";
import { openSources } from "./dialogs/sources";
import { api } from "./api";
import { drawUpdBtn, openUpdate, refreshBinary } from "./dialogs/update";
import { initServer, setStatus } from "./server";
import { initDownloads } from "./downloads";
import { renderAdvanced } from "./screens/advanced";
import { renderLibrary } from "./screens/library";
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
  const focusId = document.activeElement?.id;
  scr.replaceChildren(r === "library" ? renderLibrary() : r === "advanced" ? renderAdvanced() : renderStart());
  if (focusId && focusId !== "libq") document.getElementById(focusId)?.focus();
  setStatus();
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
  api.hardware().then((hw) => {
    S.hw = hw;
    S.useGpu = hw.gpus.map(() => true);
    render();
  }, (e) => toast(`Hardware detection failed: ${e}`));
  await refreshBinary(!!S.settings?.checkUpdatesOnStart);
  if (S.binary && !S.binary.present) toast("KoboldCpp is not installed yet. Use the button at the top to install it.");
  else if (S.update?.updateAvailable && S.update.latest) toast(`KoboldCpp ${S.update.latest.tag} is available. Use the button at the top to update.`);
  render();
}
init();

addEventListener("hashchange", render);
render();
