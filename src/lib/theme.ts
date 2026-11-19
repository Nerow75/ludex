// Thème de l'interface (réglage `theme` de `settings.rs`), pour la fenêtre principale et la
// recherche rapide. Le réglage vit côté Rust ; une copie dans localStorage l'applique dès le
// premier rendu, sans passer par le sombre le temps de la réponse.
import { onSettingsChanged, settings, THEMES, type Theme } from "./api";

const KEY = "ludex.theme";
const LIGHT = "(prefers-color-scheme: light)";

let current: Theme = "dark";

/** Thème relu (localStorage, version future…) : un thème connu, sinon le sombre. */
export function knownTheme(value: unknown): Theme {
  return typeof value === "string" && (THEMES as readonly string[]).includes(value) ? (value as Theme) : "dark";
}

/**
 * Valeur de `data-theme` sur `<html>` : `null` pour le sombre (tokens par défaut), « Comme Windows »
 * selon la préférence que Windows donne à la WebView, les autres thèmes sous leur nom.
 */
export function themeAttribute(theme: Theme, prefersLight: boolean): string | null {
  if (theme === "system") return prefersLight ? "light" : null;
  return theme === "dark" ? null : theme;
}

function remembered(): Theme {
  try {
    return knownTheme(localStorage.getItem(KEY));
  } catch {
    return "dark";
  }
}

/** Pose `data-theme` sur `<html>` (aucun attribut : sombre, les tokens par défaut). */
export function applyTheme(theme: Theme) {
  current = theme;
  try {
    localStorage.setItem(KEY, theme);
  } catch {
    // stockage indisponible : le réglage reste côté Rust
  }
  const value = themeAttribute(theme, window.matchMedia?.(LIGHT).matches ?? false);
  if (value) document.documentElement.dataset.theme = value;
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
