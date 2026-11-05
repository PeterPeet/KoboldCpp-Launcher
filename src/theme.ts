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

export function applyTheme(t: Theme = getTheme()) {
  document.documentElement.dataset.theme = t;
}

export function setTheme(t: Theme) {
  try {
    localStorage.setItem(KEY, t);
  } catch {
    /* storage unavailable: applies until restart */
  }
  applyTheme(t);
}
