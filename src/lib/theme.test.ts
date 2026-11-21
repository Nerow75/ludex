import { describe, expect, it } from "vitest";
import { ACCENT_LABEL, ACCENTS, THEMES } from "./api";
import { accentAttribute, knownAccent, knownTheme, themeAttribute } from "./theme";

describe("themeAttribute", () => {
  it("laisse le sombre sans attribut, tokens par défaut", () => {
    expect(themeAttribute("dark", false)).toBeNull();
    expect(themeAttribute("dark", true)).toBeNull();
  });

  it("pose le nom des autres thèmes, quel que soit Windows", () => {
    for (const prefersLight of [false, true]) {
      expect(themeAttribute("light", prefersLight)).toBe("light");
      expect(themeAttribute("oled", prefersLight)).toBe("oled");
      expect(themeAttribute("contrast", prefersLight)).toBe("contrast");
    }
  });

  it("suit Windows en « Comme Windows »", () => {
    expect(themeAttribute("system", true)).toBe("light");
    expect(themeAttribute("system", false)).toBeNull();
  });
});

describe("knownTheme", () => {
  it("garde chaque thème connu", () => {
    for (const t of THEMES) expect(knownTheme(t)).toBe(t);
  });

  it("revient au sombre pour une valeur inconnue ou absente", () => {
    expect(knownTheme("sepia")).toBe("dark");
    expect(knownTheme("OLED")).toBe("dark");
    expect(knownTheme(null)).toBe("dark");
    expect(knownTheme(undefined)).toBe("dark");
    expect(knownTheme(3)).toBe("dark");
  });
});

describe("accentAttribute", () => {
  it("laisse le turquoise sans attribut, tokens par défaut", () => {
    expect(accentAttribute("turquoise")).toBeNull();
  });

  it("pose le nom des autres accents", () => {
    for (const a of ACCENTS.filter((x) => x !== "turquoise")) expect(accentAttribute(a)).toBe(a);
  });
});

describe("knownAccent", () => {
  it("garde chaque accent connu, avec un libellé", () => {
    for (const a of ACCENTS) {
      expect(knownAccent(a)).toBe(a);
      expect(ACCENT_LABEL[a]).toBeTruthy();
    }
  });

  it("revient au turquoise pour une valeur inconnue ou absente", () => {
    expect(knownAccent("amber")).toBe("turquoise");
    expect(knownAccent("Blue")).toBe("turquoise");
    expect(knownAccent("")).toBe("turquoise");
    expect(knownAccent(null)).toBe("turquoise");
    expect(knownAccent(undefined)).toBe("turquoise");
    expect(knownAccent(2)).toBe("turquoise");
  });
});
