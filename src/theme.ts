// App color scheme: Light, or KoboldAI Lite's default colors. A view preference (localStorage).

export type Theme = "light" | "lite";
const KEY = "kl-theme";

export function getTheme(): Theme {
  try {
    return localStorage.getItem(KEY) === "lite" ? "lite" : "light";
  } catch {
    return "light";
  }
}

/** Esobold mode wears Esolite's colors over whatever the user chose; leaving it restores that. */
let esoOn = false;
export function setEsoTheme(on: boolean) {
  esoOn = on;
  applyTheme();
}

export function applyTheme(t: Theme = getTheme()) {
  document.documentElement.dataset.theme = esoOn ? "eso" : t;
}

export function setTheme(t: Theme) {
  try {
    localStorage.setItem(KEY, t);
  } catch {
    /* storage unavailable: applies until restart */
  }
  applyTheme(t);
}
