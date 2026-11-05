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

/** The way in: a short flicker, then the new colors (skipped with reduced motion). Each call
 *  cancels a flicker still running, so quick on/off/on never leaves an old one half-finished
 *  (a filter cut off mid-way can stay on screen in WebKit). */
let flicker: Animation | null = null;
const ENTER: Keyframe[] = [
  { offset: 0, filter: "none", transform: "none" },
  { offset: 0.12, filter: "hue-rotate(140deg) contrast(1.8) brightness(1.4)", transform: "translateX(-4px)" },
  { offset: 0.2, filter: "invert(1) hue-rotate(180deg)", transform: "translateX(5px) skewX(-2deg)" },
  { offset: 0.28, filter: "brightness(0.2)", transform: "none" },
  { offset: 0.4, filter: "hue-rotate(90deg) saturate(2)", transform: "translateX(-2px)" },
  { offset: 0.55, filter: "brightness(0.6) contrast(1.3)", transform: "none" },
  { offset: 1, filter: "none", transform: "none" },
];
const LEAVE: Keyframe[] = [{ filter: "brightness(0.3) saturate(0)" }, { filter: "none" }];

export function enterAnimation(on: boolean) {
  flicker?.cancel();
  flicker = null;
  if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches || !document.body.animate) return;
  const a = document.body.animate(on ? ENTER : LEAVE, { duration: on ? 850 : 600, iterations: on ? 2 : 1, easing: "ease-out", fill: "none" });
  flicker = a;
  a.onfinish = a.oncancel = () => {
    if (flicker === a) flicker = null;
  };
}
