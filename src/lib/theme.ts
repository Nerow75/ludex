// Thème et couleur d'accent de l'interface (réglages `theme` et `accent` de `settings.rs`), pour la
// fenêtre principale et la recherche rapide. Les réglages vivent côté Rust ; une copie dans
// localStorage les applique dès le premier rendu, sans passer par le sombre turquoise le temps de
// la réponse.
import { ACCENTS, onSettingsChanged, settings, THEMES, type Accent, type Theme } from "./api";

const KEY = "ludex.theme";
const ACCENT_KEY = "ludex.accent";
const LIGHT = "(prefers-color-scheme: light)";

let current: Theme = "dark";

/** Thème relu (localStorage, version future…) : un thème connu, sinon le sombre. */
export function knownTheme(value: unknown): Theme {
  return typeof value === "string" && (THEMES as readonly string[]).includes(value) ? (value as Theme) : "dark";
}

/** Accent relu (localStorage, version future…) : un accent connu, sinon le turquoise. */
export function knownAccent(value: unknown): Accent {
  return typeof value === "string" && (ACCENTS as readonly string[]).includes(value) ? (value as Accent) : "turquoise";
}

/**
 * Valeur de `data-theme` sur `<html>` : `null` pour le sombre (tokens par défaut), « Comme Windows »
 * selon la préférence que Windows donne à la WebView, les autres thèmes sous leur nom.
 */
export function themeAttribute(theme: Theme, prefersLight: boolean): string | null {
  if (theme === "system") return prefersLight ? "light" : null;
  return theme === "dark" ? null : theme;
}

/** Valeur de `data-accent` sur `<html>` : `null` pour le turquoise (tokens par défaut), sinon son nom. */
export function accentAttribute(accent: Accent): string | null {
  return accent === "turquoise" ? null : accent;
}

function remember(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // stockage indisponible : le réglage reste côté Rust
  }
}

function recall(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function setAttribute(name: "theme" | "accent", value: string | null) {
  if (value) document.documentElement.dataset[name] = value;
  else delete document.documentElement.dataset[name];
}

/** Pose `data-theme` sur `<html>` (aucun attribut : sombre, les tokens par défaut). */
export function applyTheme(theme: Theme) {
  current = theme;
  remember(KEY, theme);
  setAttribute("theme", themeAttribute(theme, window.matchMedia?.(LIGHT).matches ?? false));
}

/** Pose `data-accent` sur `<html>` (aucun attribut : turquoise, les tokens par défaut). */
export function applyAccent(accent: Accent) {
  remember(ACCENT_KEY, accent);
  setAttribute("accent", accentAttribute(accent));
}

/** À appeler avant le premier rendu : thème et accent retenus, puis ceux des réglages et leurs changements. */
export function startTheme() {
  applyTheme(knownTheme(recall(KEY)));
  applyAccent(knownAccent(recall(ACCENT_KEY)));
  const apply = (s: { theme: Theme; accent: Accent }) => {
    applyTheme(s.theme);
    applyAccent(knownAccent(s.accent));
  };
  settings()
    .then(apply)
    .catch(() => {}); // sans réponse, le thème retenu reste
  onSettingsChanged(apply).catch(() => {});
  // Windows passe du clair au sombre (ou l'inverse) : suivi seulement en « Comme Windows ».
  window.matchMedia?.(LIGHT).addEventListener("change", () => current === "system" && applyTheme("system"));
}
