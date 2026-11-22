// Thème et couleur d'accent de l'interface (réglages `theme` et `accent` de `settings.rs`), pour la
// fenêtre principale et la recherche rapide. Les réglages vivent côté Rust ; une copie dans
// localStorage les applique dès le premier rendu, sans passer par le sombre turquoise le temps de
// la réponse.
import { ACCENTS, onSettingsChanged, settings, THEMES, type Accent, type Theme } from "./api";
import { baseAttribute, deriveTheme, knownCustomTheme, setThemeVars, type CustomTheme } from "./customTheme";

const KEY = "ludex.theme";
const ACCENT_KEY = "ludex.accent";
const CUSTOM_KEY = "ludex.customTheme";
const LIGHT = "(prefers-color-scheme: light)";

let current: Theme = "dark";
/** Thème perso appliqué : il décide de `data-theme` (sa base) tant qu'il est là. */
let custom: CustomTheme | null = null;

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
  if (custom) return;
  setAttribute("theme", themeAttribute(theme, window.matchMedia?.(LIGHT).matches ?? false));
}

/**
 * Pose un thème perso sur `<html>` (variables CSS calculées, `data-theme` de sa base) ; `null` le
 * retire et rend le thème de base retenu.
 */
export function applyCustomTheme(theme: CustomTheme | null) {
  custom = theme;
  try {
    if (theme) localStorage.setItem(CUSTOM_KEY, JSON.stringify(theme));
    else localStorage.removeItem(CUSTOM_KEY);
  } catch {
    // stockage indisponible : le réglage reste côté Rust
  }
  const root = document.documentElement;
  if (!theme) {
    setThemeVars(root, null);
    applyTheme(current);
    return;
  }
  setThemeVars(root, deriveTheme(theme.base, theme));
  setAttribute("theme", baseAttribute(theme.base));
}

function recallCustom(): CustomTheme | null {
  try {
    return knownCustomTheme(JSON.parse(recall(CUSTOM_KEY) ?? "null"));
  } catch {
    return null;
  }
}

/** Thème perso actif des réglages (id connu de la liste), sinon `null`. */
export function activeCustomTheme(s: { customTheme: string | null; customThemes: CustomTheme[] }): CustomTheme | null {
  return s.customTheme ? s.customThemes.find((t) => t.id === s.customTheme) ?? null : null;
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
  const kept = recallCustom();
  if (kept) applyCustomTheme(kept);
  const apply = (s: { theme: Theme; accent: Accent; customTheme: string | null; customThemes: CustomTheme[] }) => {
    applyTheme(s.theme);
    applyAccent(knownAccent(s.accent));
    const next = activeCustomTheme(s);
    if (JSON.stringify(next) !== JSON.stringify(custom)) applyCustomTheme(next);
  };
  settings()
    .then(apply)
    .catch(() => {}); // sans réponse, le thème retenu reste
  onSettingsChanged(apply).catch(() => {});
  // Windows passe du clair au sombre (ou l'inverse) : suivi seulement en « Comme Windows ».
  window.matchMedia?.(LIGHT).addEventListener("change", () => current === "system" && !custom && applyTheme("system"));
}
