// KoboldCpp or Esobold mode: names, links, logo, colors and window title follow the mode.
// Esobold is a community fork of KoboldCpp (with Esolite instead of Lite); the user switches to
// it in Advanced setup › Esobold. Rust keeps a separate install and release source for it.

import { $ } from "./dom";
import { S } from "./state";
import { setEsoTheme } from "./theme";
import koboldLogo from "./assets/koboldai-logo.png";
import esoLogo from "./assets/esobold-niko.png";

export const eso = () => !!S.settings?.esobold;
/** The program the launcher runs: "KoboldCpp" or "Esobold". */
export const progName = () => (eso() ? "Esobold" : "KoboldCpp");
/** Its web UI: "Lite" or "Esolite". */
export const liteName = () => (eso() ? "Esolite" : "Lite");
export const repoUrl = () => (eso() ? "https://github.com/esolithe/esobold" : "https://github.com/LostRuins/koboldcpp");

/** The phrase that unlocks the switch (case and surrounding spaces don't matter). */
export const MANTRA = "I know what I am doing";
export const mantraOk = (s: string) => s.trim().replace(/\s+/g, " ").toLowerCase() === MANTRA.toLowerCase();

/** Update everything outside the screens that depends on the mode. */
export function applyFlavor() {
  const on = eso();
  document.documentElement.dataset.eso = on ? "on" : "";
  setEsoTheme(on);
  const brand = $<HTMLAnchorElement>(".brand");
  if (brand) {
    brand.href = on ? "https://github.com/esolithe/esobold" : "https://koboldai.com";
    brand.title = on ? "Esobold on GitHub" : "koboldai.com";
    $<HTMLImageElement>(".klogo", brand)!.src = on ? esoLogo : koboldLogo;
    $("#brandName", brand)!.textContent = progName();
  }
  const ver = $<HTMLAnchorElement>(".verlink");
  if (ver) {
    ver.href = repoUrl();
    ver.title = `Open the ${progName()} repository on GitHub`;
  }
  const lite = $("#liteBtn");
  if (lite) {
    lite.textContent = `Open ${liteName()}`;
    lite.title = `Open ${on ? "Esolite" : "KoboldAI Lite"} in your browser`;
  }
  const title = on ? "KoboldCpp Peter's Launcher · Esobold mode" : "KoboldCpp Peter's Launcher";
  document.title = title;
  import("@tauri-apps/api/window").then((w) => w.getCurrentWindow().setTitle(title)).catch(() => {});
}

/** The way in: a short flicker, then the new colors (skipped with reduced motion). */
export function enterAnimation(on: boolean) {
  const root = document.documentElement;
  root.classList.remove("eso-enter", "eso-leave");
  void root.offsetWidth;
  root.classList.add(on ? "eso-enter" : "eso-leave");
  window.setTimeout(() => root.classList.remove("eso-enter", "eso-leave"), 900);
}
