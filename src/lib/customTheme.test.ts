import { describe, expect, it } from "vitest";
import {
  BASE_COLORS,
  CUSTOM_VARS,
  THEME_BASES,
  baseAttribute,
  cleanHex,
  cleanThemeName,
  contrast,
  deriveTheme,
  ensureContrast,
  isDark,
  knownCustomTheme,
  minContrast,
  mix,
  parseShareCode,
  shareCode,
  textTarget,
} from "./customTheme";

/** Les seuils du design system, vérifiés sur des tokens calculés. */
function expectReadable(base: (typeof THEME_BASES)[number], vars: Record<string, string>) {
  const backs = [vars["--lx-bg"], vars["--lx-surface"], vars["--lx-surface-2"]];
  const T = textTarget(base);
  for (const name of ["--lx-text", "--lx-text-2", "--lx-text-3", "--lx-accent-text", "--lx-success", "--lx-warning", "--lx-danger"]) {
    expect(minContrast(vars[name], backs), name).toBeGreaterThanOrEqual(T);
  }
  expect(minContrast(vars["--lx-accent"], backs)).toBeGreaterThanOrEqual(3);
  expect(minContrast(vars["--lx-focus"], backs)).toBeGreaterThanOrEqual(3);
  expect(contrast(vars["--lx-on-accent"], vars["--lx-accent"])).toBeGreaterThanOrEqual(T);
  expect(contrast(vars["--lx-text"], vars["--lx-accent-soft"])).toBeGreaterThanOrEqual(T);
}

describe("couleurs", () => {
  it("n'accepte que #RRGGBB", () => {
    expect(cleanHex("#0b0b0b")).toBe("#0B0B0B");
    expect(cleanHex(" 14b8a6 ")).toBe("#14B8A6");
    for (const bad of ["", "#FFF", "red", "#12345G", "#1234567", "var(--x)", "#000000;color:red", "##000000"]) expect(cleanHex(bad), bad).toBeNull();
  });

  it("calcule les rapports WCAG", () => {
    expect(contrast("#000000", "#FFFFFF")).toBeCloseTo(21, 5);
    expect(contrast("#FFFFFF", "#FFFFFF")).toBe(1);
    // Valeurs notées dans tokens.css : turquoise du sombre sur #242424, 6,2:1 ; texte discret du clair 4,8:1.
    expect(contrast("#14B8A6", "#242424")).toBeCloseTo(6.2, 1);
    expect(contrast("#666666", "#EBEBEB")).toBeCloseTo(4.8, 1);
    expect(isDark("#0B0B0B") && isDark("#6B6B6B") && !isDark("#A0A0A0") && !isDark("#F5F0E6")).toBe(true);
    expect(mix("#000000", "#FFFFFF", 0.5)).toBe("#808080");
  });

  it("corrige une couleur au plus près, sinon le signale", () => {
    const same = ensureContrast("#FFFFFF", ["#0B0B0B"], 4.5);
    expect(same).toMatchObject({ hex: "#FFFFFF", ok: true, changed: false });
    // Accent trop sombre sur un fond sombre : éclairci, même teinte.
    const fixed = ensureContrast("#1E3A8A", ["#0B0B0B", "#1A1A1A", "#242424"], 3);
    expect(fixed.ok && fixed.changed).toBe(true);
    expect(minContrast(fixed.hex, ["#0B0B0B", "#1A1A1A", "#242424"])).toBeGreaterThanOrEqual(3);
    expect(fixed.ratio).toBeLessThan(3.3); // pas plus loin que nécessaire
    // Fond noir et surface blanche : au mieux un gris à 4,58:1 sur les deux, jamais 7:1.
    expect(ensureContrast("#808080", ["#000000", "#FFFFFF"], 4.5).ok).toBe(true);
    expect(ensureContrast("#808080", ["#000000", "#FFFFFF"], 7).ok).toBe(false);
  });
});

describe("thème calculé", () => {
  it("garde les valeurs des thèmes d'origine pour leurs couleurs de base", () => {
    const dark = deriveTheme("dark", BASE_COLORS.dark);
    expect(dark.ok).toBe(true);
    expect(dark.scheme).toBe("dark");
    expect(dark.vars["--lx-accent"]).toBe("#14B8A6");
    expect(dark.vars["--lx-text"]).toBe("#FFFFFF");
    expect(dark.checks.every((c) => c.status === "ok")).toBe(true);
    expect(deriveTheme("light", BASE_COLORS.light).scheme).toBe("light");
  });

  it("tient tous les seuils, quelle que soit la base", () => {
    for (const base of THEME_BASES) {
      const d = deriveTheme(base, BASE_COLORS[base]);
      expect(d.ok, base).toBe(true);
      expectReadable(base, d.vars);
      expect(Object.keys(d.vars).sort()).toEqual([...CUSTOM_VARS].sort());
    }
  });

  it("corrige des couleurs mal choisies et le dit", () => {
    // Texte gris sur fond sombre, accent bleu nuit : texte et accent corrigés.
    const d = deriveTheme("dark", { bg: "#101418", surface: "#1B2128", text: "#555555", accent: "#1E3A8A" });
    expect(d.ok).toBe(true);
    expectReadable("dark", d.vars);
    const text = d.checks.find((c) => c.key === "text")!;
    expect(text).toMatchObject({ status: "fixed", from: "#555555" });
    expect(d.checks.find((c) => c.key === "accent")!.status).toBe("fixed");
    // Papier clair, accent jaune pâle.
    const paper = deriveTheme("light", { bg: "#F5F0E6", surface: "#EAE3D6", text: "#3A3A3A", accent: "#F5D76E" });
    expect(paper.ok && paper.scheme === "light").toBe(true);
    expectReadable("light", paper.vars);
  });

  it("contraste élevé : 7:1 pour les textes", () => {
    const d = deriveTheme("contrast", { bg: "#000000", surface: "#101010", text: "#C0C0C0", accent: "#8B5CF6" });
    expect(d.ok).toBe(true);
    expectReadable("contrast", d.vars);
    expect(d.vars["--lx-border"]).not.toBe(d.vars["--lx-surface"]);
    expect(minContrast(d.vars["--lx-border"], [d.vars["--lx-bg"], d.vars["--lx-surface"]])).toBeGreaterThanOrEqual(3);
  });

  it("refuse un fond et des surfaces que rien ne rend lisibles", () => {
    const d = deriveTheme("dark", { bg: "#000000", surface: "#FFFFFF", text: "#888888", accent: "#14B8A6" });
    expect(d.ok).toBe(false);
    expect(d.checks.some((c) => c.status === "fail")).toBe(true);
  });

  it("donne data-theme de la base", () => {
    expect(baseAttribute("dark")).toBeNull();
    expect(baseAttribute("contrast")).toBe("contrast");
  });
});

describe("relecture et partage", () => {
  const theme = { id: "t-1", name: "Nuit d'été", base: "oled" as const, bg: "#000000", surface: "#0A0A0A", text: "#FFFFFF", accent: "#FF8800" };

  it("relit un thème gardé, refuse le reste", () => {
    expect(knownCustomTheme(theme)).toEqual(theme);
    expect(knownCustomTheme({ ...theme, bg: "red" })).toBeNull();
    expect(knownCustomTheme({ ...theme, base: "sepia" })).toBeNull();
    expect(knownCustomTheme(null)).toBeNull();
    expect(knownCustomTheme("x")).toBeNull();
  });

  it("nettoie le nom comme Rust", () => {
    expect(cleanThemeName("  Mon ​thème\n nuit ")).toBe("Mon thème nuit");
    expect([...cleanThemeName("x".repeat(80))].length).toBe(40);
  });

  it("fait l'aller-retour en texte court", () => {
    const code = shareCode(theme);
    expect(code).toBe("LUDEX:1:oled:000000:0A0A0A:FFFFFF:FF8800:Nuit d'été");
    expect(parseShareCode(`  ${code}\n`)).toEqual({ name: "Nuit d'été", base: "oled", bg: "#000000", surface: "#0A0A0A", text: "#FFFFFF", accent: "#FF8800" });
    // Deux-points dans le nom gardés.
    expect(parseShareCode("ludex:1:dark:0b0b0b:1a1a1a:ffffff:14b8a6:A: B")?.name).toBe("A: B");
  });

  it("refuse un texte de partage invalide", () => {
    for (const bad of [
      "",
      "Nuit",
      "LUDEX:2:dark:000000:0A0A0A:FFFFFF:FF8800:X",
      "LUDEX:1:sepia:000000:0A0A0A:FFFFFF:FF8800:X",
      "LUDEX:1:dark:000000:0A0A0A:FFFFFF:url(x):X",
      "LUDEX:1:dark:000000:0A0A0A:FFFFFF:FF8800",
      "LUDEX:1:dark:000000:0A0A0A:FFFFFF:FF8800:​",
      `LUDEX:1:dark:000000:0A0A0A:FFFFFF:FF8800:${"x".repeat(200)}`,
    ]) {
      expect(parseShareCode(bad), bad).toBeNull();
    }
  });
});
