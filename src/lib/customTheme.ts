// Thèmes perso (`custom_theme.rs`) : quatre couleurs choisies (fond, surfaces, texte, accent) sur
// un thème de base ; tous les autres tokens `--lx-*` en sont tirés ici, et les contrastes exigés
// par le design system sont vérifiés et corrigés (texte 4,5:1, 7:1 en contraste élevé ; accent et
// focus 3:1 sur le fond et les deux surfaces). Fonctions pures, sans DOM (testées par Vitest),
// sauf `applyCustomTheme`.

/** Thème de base d'un thème perso. */
export type ThemeBase = "dark" | "light" | "oled" | "contrast";
export const THEME_BASES: readonly ThemeBase[] = ["dark", "light", "oled", "contrast"];

/** Thème perso tel que Rust le garde (`CustomTheme`). */
export interface CustomTheme {
  id: string;
  name: string;
  base: ThemeBase;
  bg: string;
  surface: string;
  text: string;
  accent: string;
}

/** Les quatre couleurs choisies. */
export type ThemeColors = Pick<CustomTheme, "bg" | "surface" | "text" | "accent">;
export const COLOR_KEYS: readonly (keyof ThemeColors)[] = ["bg", "surface", "text", "accent"];

/** Couleurs de départ de chaque base : celles de `tokens.css` (accent turquoise). */
export const BASE_COLORS: Record<ThemeBase, ThemeColors> = {
  dark: { bg: "#0B0B0B", surface: "#1A1A1A", text: "#FFFFFF", accent: "#14B8A6" },
  light: { bg: "#FFFFFF", surface: "#F5F5F5", text: "#0B0B0B", accent: "#0D9488" },
  oled: { bg: "#000000", surface: "#0A0A0A", text: "#FFFFFF", accent: "#14B8A6" },
  contrast: { bg: "#000000", surface: "#0F0F0F", text: "#FFFFFF", accent: "#2DD4BF" },
};

export interface Rgb {
  r: number;
  g: number;
  b: number;
}

/** `#RRGGBB` (dièse facultatif, casse ignorée), en majuscules ; `null` sinon. Même règle que Rust. */
export function cleanHex(raw: string): string | null {
  const digits = raw.trim().replace(/^#/, "");
  return /^[0-9a-fA-F]{6}$/.test(digits) ? `#${digits.toUpperCase()}` : null;
}

export function parseHex(hex: string): Rgb {
  const h = cleanHex(hex) ?? "#000000";
  return { r: parseInt(h.slice(1, 3), 16), g: parseInt(h.slice(3, 5), 16), b: parseInt(h.slice(5, 7), 16) };
}

export function toHex({ r, g, b }: Rgb): string {
  const part = (v: number) => Math.round(Math.min(255, Math.max(0, v))).toString(16).padStart(2, "0");
  return `#${part(r)}${part(g)}${part(b)}`.toUpperCase();
}

/** Luminance relative WCAG 2.x. */
export function luminance(hex: string): number {
  const { r, g, b } = parseHex(hex);
  const c = (v: number) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * c(r) + 0.7152 * c(g) + 0.0722 * c(b);
}

/** Rapport de contraste WCAG entre deux couleurs (1 à 21). */
export function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/** Le plus faible des contrastes d'une couleur sur plusieurs fonds. */
export function minContrast(color: string, backgrounds: readonly string[]): number {
  return Math.min(...backgrounds.map((b) => contrast(color, b)));
}

/** Fond sombre : le blanc y contraste plus que le noir (même règle que `is_dark` de Rust). */
export function isDark(hex: string): boolean {
  return contrast(hex, "#FFFFFF") >= contrast(hex, "#000000");
}

/** Mélange en sRGB, comme `color-mix(in srgb, a (1 - t), b t)`. */
export function mix(a: string, b: string, t: number): string {
  const x = parseHex(a);
  const y = parseHex(b);
  return toHex({ r: x.r + (y.r - x.r) * t, g: x.g + (y.g - x.g) * t, b: x.b + (y.b - x.b) * t });
}

function toHsl(hex: string): [number, number, number] {
  const { r, g, b } = parseHex(hex);
  const [R, G, B] = [r / 255, g / 255, b / 255];
  const max = Math.max(R, G, B);
  const min = Math.min(R, G, B);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  const h = max === R ? (G - B) / d + (G < B ? 6 : 0) : max === G ? (B - R) / d + 2 : (R - G) / d + 4;
  return [h / 6, s, l];
}

function fromHsl(h: number, s: number, l: number): string {
  if (s === 0) return toHex({ r: l * 255, g: l * 255, b: l * 255 });
  const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
  const p = 2 * l - q;
  const hue = (t: number) => {
    const u = t < 0 ? t + 1 : t > 1 ? t - 1 : t;
    if (u < 1 / 6) return p + (q - p) * 6 * u;
    if (u < 1 / 2) return q;
    if (u < 2 / 3) return p + (q - p) * (2 / 3 - u) * 6;
    return p;
  };
  return toHex({ r: hue(h + 1 / 3) * 255, g: hue(h) * 255, b: hue(h - 1 / 3) * 255 });
}

export interface Fixed {
  /** Couleur retenue (celle d'origine si elle suffisait). */
  hex: string;
  /** Contraste le plus faible obtenu. */
  ratio: number;
  /** Seuil atteint. */
  ok: boolean;
  /** Couleur changée pour atteindre le seuil. */
  changed: boolean;
}

/**
 * Couleur éclaircie ou assombrie (teinte et saturation gardées) jusqu'à `target` sur chaque fond,
 * au plus près de l'originale ; sans solution, la plus lisible trouvée (`ok` faux).
 */
export function ensureContrast(color: string, backgrounds: readonly string[], target: number): Fixed {
  const hex = cleanHex(color) ?? "#000000";
  const start = minContrast(hex, backgrounds);
  if (start >= target) return { hex, ratio: start, ok: true, changed: false };
  const [h, s, l] = toHsl(hex);
  let best: Fixed = { hex, ratio: start, ok: false, changed: false };
  let found: { hex: string; ratio: number; step: number } | null = null;
  for (const dir of [1, -1]) {
    for (let step = 1; step <= 100; step++) {
      const nl = l + (dir * step) / 100;
      if (nl < 0 || nl > 1) break;
      const candidate = fromHsl(h, s, nl);
      const ratio = minContrast(candidate, backgrounds);
      if (ratio > best.ratio) best = { hex: candidate, ratio, ok: false, changed: true };
      if (ratio >= target) {
        if (!found || step < found.step) found = { hex: candidate, ratio, step };
        break;
      }
    }
  }
  if (found) return { hex: found.hex, ratio: found.ratio, ok: true, changed: true };
  // Teinte trop claire ou trop sombre pour y arriver : noir ou blanc, s'ils suffisent.
  for (const plain of ["#000000", "#FFFFFF"]) {
    const ratio = minContrast(plain, backgrounds);
    if (ratio >= target) return { hex: plain, ratio, ok: true, changed: true };
  }
  return best;
}

/** Contrôle affiché dans l'éditeur. */
export interface ContrastCheck {
  key: "text" | "muted" | "accent" | "onAccent" | "accentText" | "focus";
  ratio: number;
  target: number;
  /** `ok` tel quel, `fixed` corrigé, `fail` impossible (thème refusé). */
  status: "ok" | "fixed" | "fail";
  /** Couleur choisie, puis retenue (seulement si corrigée). */
  from?: string;
  to?: string;
}

export interface DerivedTheme {
  /** Variables CSS à poser sur `<html>` (noms complets `--lx-*`). */
  vars: Record<string, string>;
  scheme: "dark" | "light";
  checks: ContrastCheck[];
  /** Tous les seuils atteints : le thème peut être enregistré. */
  ok: boolean;
}

/** Variables posées par un thème perso : retirées toutes ensemble. */
export const CUSTOM_VARS = [
  "--lx-bg", "--lx-surface", "--lx-surface-2", "--lx-border", "--lx-text", "--lx-text-2", "--lx-text-3",
  "--lx-success", "--lx-warning", "--lx-danger", "--lx-accent", "--lx-on-accent", "--lx-accent-text",
  "--lx-accent-soft", "--lx-focus", "--lx-overlay",
] as const;

/** Couleurs d'état d'origine (sombre, clair, contraste élevé), corrigées au besoin. */
const STATE_COLORS = {
  dark: ["#5EE08F", "#F3BD55", "#FF7A7A"],
  light: ["#137036", "#A14D07", "#C21F1F"],
  contrast: ["#7CF5A8", "#FFD27A", "#FF9B9B"],
} as const;

/** Seuil des textes : 4,5:1, 7:1 (AAA) en contraste élevé. */
export const textTarget = (base: ThemeBase) => (base === "contrast" ? 7 : 4.5);

/** Tous les tokens d'un thème perso, avec les contrôles de contraste. */
export function deriveTheme(base: ThemeBase, colors: ThemeColors): DerivedTheme {
  const bg = cleanHex(colors.bg) ?? BASE_COLORS[base].bg;
  const surface = cleanHex(colors.surface) ?? BASE_COLORS[base].surface;
  const chosenText = cleanHex(colors.text) ?? BASE_COLORS[base].text;
  const chosenAccent = cleanHex(colors.accent) ?? BASE_COLORS[base].accent;
  const dark = isDark(bg);
  const T = textTarget(base);
  const checks: ContrastCheck[] = [];
  const note = (key: ContrastCheck["key"], fixed: Fixed, target: number, from: string) =>
    checks.push({ key, ratio: fixed.ratio, target, status: !fixed.ok ? "fail" : fixed.changed ? "fixed" : "ok", ...(fixed.changed ? { from, to: fixed.hex } : {}) });

  // Deuxième surface (champs, survols) et contours : un pas de la surface vers le texte.
  const surface2 = mix(surface, chosenText, 0.045);
  const backs = [bg, surface, surface2];
  const text = ensureContrast(chosenText, backs, T);
  note("text", text, T, chosenText);
  // Textes secondaire et discret jamais plus marqués que le texte principal (texte corrigé au seuil :
  // tous les trois au seuil plutôt qu'une hiérarchie inversée).
  const capped = (f: Fixed): Fixed => (minContrast(f.hex, backs) > minContrast(text.hex, backs) ? { ...f, hex: text.hex, ratio: text.ratio } : f);
  const text2 = capped(ensureContrast(mix(text.hex, bg, 0.15), backs, T)).hex;
  const muted = capped(ensureContrast(mix(text.hex, bg, 0.45), backs, T));
  note("muted", muted, T, mix(text.hex, bg, 0.45));
  // Contraste élevé : contours visibles à 3:1 comme ceux de la palette d'origine.
  const border = base === "contrast" ? ensureContrast(mix(surface, text.hex, 0.6), backs, 3).hex : mix(surface, text.hex, 0.12);

  // Accent : remplissages et focus à 3:1 sur les trois fonds.
  let accent = ensureContrast(chosenAccent, backs, 3);
  // Texte posé sur l'accent : le fond ou le texte du thème s'ils suffisent, sinon noir ou blanc.
  const pickOn = (fill: string) =>
    [bg, text.hex, "#000000", "#FFFFFF"].map((c) => ({ c, r: contrast(c, fill) })).find((x) => x.r >= T) ??
    [{ c: "#000000", r: contrast("#000000", fill) }, { c: "#FFFFFF", r: contrast("#FFFFFF", fill) }].sort((a, b) => b.r - a.r)[0];
  let on = pickOn(accent.hex);
  if (on.r < T) {
    // Accent de teinte moyenne en contraste élevé : poussé vers le clair ou le sombre.
    for (const plain of ["#000000", "#FFFFFF"]) {
      const pushed = ensureContrast(accent.hex, [plain], T);
      if (pushed.ok && minContrast(pushed.hex, backs) >= 3) {
        accent = { ...accent, hex: pushed.hex, ratio: minContrast(pushed.hex, backs), changed: true };
        on = { c: plain, r: pushed.ratio };
        break;
      }
    }
  }
  note("accent", accent, 3, chosenAccent);
  checks.push({ key: "onAccent", ratio: on.r, target: T, status: on.r >= T ? "ok" : "fail" });
  const accentText = ensureContrast(chosenAccent, backs, T);
  note("accentText", accentText, T, chosenAccent);
  // Focus = l'accent retenu (3:1 sur les trois fonds).
  checks.push({ key: "focus", ratio: minContrast(accent.hex, backs), target: 3, status: accent.ok ? "ok" : "fail" });
  // Pastille cochée sous le texte : un fond teinté par l'accent, au contraste du texte.
  const soft = ensureContrast(mix(accent.hex, bg, dark ? 0.72 : 0.82), [text.hex], T).hex;

  const states = STATE_COLORS[base === "contrast" && dark ? "contrast" : dark ? "dark" : "light"].map((c) => ensureContrast(c, backs, T).hex);
  const { r, g, b } = parseHex(bg);
  const overlay = `rgba(${r}, ${g}, ${b}, ${base === "contrast" ? 0.88 : base === "oled" ? 0.72 : 0.6})`;

  return {
    vars: {
      "--lx-bg": bg,
      "--lx-surface": surface,
      "--lx-surface-2": surface2,
      "--lx-border": border,
      "--lx-text": text.hex,
      "--lx-text-2": text2,
      "--lx-text-3": muted.hex,
      "--lx-success": states[0],
      "--lx-warning": states[1],
      "--lx-danger": states[2],
      "--lx-accent": accent.hex,
      "--lx-on-accent": on.c,
      "--lx-accent-text": accentText.hex,
      "--lx-accent-soft": soft,
      "--lx-focus": accent.hex,
      "--lx-overlay": overlay,
    },
    scheme: dark ? "dark" : "light",
    checks,
    ok: checks.every((c) => c.status !== "fail"),
  };
}

/** Valeur de `data-theme` d'un thème perso : celle de sa base (règles en plus d'`app.css`). */
export function baseAttribute(base: ThemeBase): string | null {
  return base === "dark" ? null : base;
}

/** Thème relu (localStorage, version future…) : complet et valide, sinon `null`. */
export function knownCustomTheme(value: unknown): CustomTheme | null {
  if (!value || typeof value !== "object") return null;
  const v = value as Record<string, unknown>;
  if (typeof v.id !== "string" || typeof v.name !== "string" || !THEME_BASES.includes(v.base as ThemeBase)) return null;
  const colors = COLOR_KEYS.map((k) => (typeof v[k] === "string" ? cleanHex(v[k] as string) : null));
  if (colors.some((c) => !c)) return null;
  const [bg, surface, text, accent] = colors as string[];
  return { id: v.id, name: v.name, base: v.base as ThemeBase, bg, surface, text, accent };
}

// ---------- Partage en texte court ----------
// « LUDEX:1:<base>:<fond>:<surface>:<texte>:<accent>:<nom> », couleurs sans dièse. Le texte collé
// est revérifié ici, puis par Rust à l'enregistrement.

const SHARE_PREFIX = "LUDEX:1:";
/** Au-delà, un texte collé est refusé sans être lu. */
/** Thèmes gardés au plus (`MAX_THEMES` de `custom_theme.rs`). */
export const MAX_THEMES = 20;
export const MAX_SHARE = 160;
/** Longueur d'un nom de thème (même plafond que Rust). */
export const MAX_THEME_NAME = 40;
/** Caractères invisibles retirés (mêmes plages que `is_invisible` de Rust). */
const invisible = (c: number) => (c >= 0x200b && c <= 0x200f) || (c >= 0x202a && c <= 0x202e) || (c >= 0x2060 && c <= 0x206f) || c === 0xfeff;
/** Contrôles : séparent les mots, comme côté Rust. */
const control = (c: number) => c <= 0x1f || (c >= 0x7f && c <= 0x9f);

/** Nom nettoyé comme côté Rust (contrôles et invisibles retirés, espaces regroupés, 40 caractères). */
export function cleanThemeName(raw: string): string {
  const visible = [...raw]
    .map((ch) => {
      const c = ch.codePointAt(0) ?? 0;
      return invisible(c) ? "" : control(c) ? " " : ch;
    })
    .join("");
  return [...visible.split(/\s+/).filter(Boolean).join(" ")].slice(0, MAX_THEME_NAME).join("").trimEnd();
}

export function shareCode(theme: Pick<CustomTheme, "name" | "base"> & ThemeColors): string {
  const colors = COLOR_KEYS.map((k) => (cleanHex(theme[k]) ?? "#000000").slice(1));
  return `${SHARE_PREFIX}${theme.base}:${colors.join(":")}:${cleanThemeName(theme.name)}`;
}

/** Thème lu dans un texte de partage ; `null` s'il est illisible. */
export function parseShareCode(raw: string): (Pick<CustomTheme, "name" | "base"> & ThemeColors) | null {
  if (raw.length > MAX_SHARE) return null;
  const text = raw.trim();
  if (!text.toUpperCase().startsWith(SHARE_PREFIX)) return null;
  const parts = text.slice(SHARE_PREFIX.length).split(":");
  if (parts.length < 6) return null;
  const [base, ...rest] = parts;
  const lower = base.toLowerCase() as ThemeBase;
  if (!THEME_BASES.includes(lower)) return null;
  const colors = rest.slice(0, 4).map((c) => cleanHex(c));
  if (colors.some((c) => !c)) return null;
  const name = cleanThemeName(rest.slice(4).join(":"));
  if (!name) return null;
  const [bg, surface, text2, accent] = colors as string[];
  return { name, base: lower, bg, surface, text: text2, accent };
}

/** Pose les variables d'un thème perso sur un élément (`null` : les retire). */
export function setThemeVars(el: HTMLElement, derived: DerivedTheme | null) {
  for (const name of CUSTOM_VARS) {
    if (derived) el.style.setProperty(name, derived.vars[name]);
    else el.style.removeProperty(name);
  }
  if (derived) el.style.setProperty("color-scheme", derived.scheme);
  else el.style.removeProperty("color-scheme");
}
