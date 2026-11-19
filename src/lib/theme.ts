// Thème de l'interface (réglage `theme` de `settings.rs`), pour la fenêtre principale et la
// recherche rapide. Le réglage vit côté Rust ; une copie dans localStorage l'applique dès le
// premier rendu, sans passer par le sombre le temps de la réponse.
import { onSettingsChanged, settings, THEMES, type Theme } from "./api";

const KEY = "ludex.theme";
const LIGHT = "(prefers-color-scheme: light)";

let current: Theme = "dark";

function remembered(): Theme {
  try {
    const v = localStorage.getItem(KEY) as Theme | null;
    return v && THEMES.includes(v) ? v : "dark";
  } catch {
    return "dark";
  }
}

/** Thème réellement affiché : « Comme Windows » suit la préférence que Windows donne à la WebView. */
export function resolvedTheme(theme: Theme): "dark" | "light" {
  if (theme !== "system") return theme;
  return window.matchMedia?.(LIGHT).matches ? "light" : "dark";
}

/** Pose `data-theme` sur `<html>` (aucun attribut : sombre, les tokens par défaut). */
export function applyTheme(theme: Theme) {
  current = theme;
  try {
    localStorage.setItem(KEY, theme);
  } catch {
    // stockage indisponible : le réglage reste côté Rust
  }
  if (resolvedTheme(theme) === "light") document.documentElement.dataset.theme = "light";
  else delete document.documentElement.dataset.theme;
}

/** À appeler avant le premier rendu : thème retenu, puis celui des réglages et ses changements. */
export function startTheme() {
  applyTheme(remembered());
  settings()
    .then((s) => applyTheme(s.theme))
    .catch(() => {}); // sans réponse, le thème retenu reste
  onSettingsChanged((s) => applyTheme(s.theme)).catch(() => {});
  // Windows passe du clair au sombre (ou l'inverse) : suivi seulement en « Comme Windows ».
  window.matchMedia?.(LIGHT).addEventListener("change", () => current === "system" && applyTheme("system"));
}
