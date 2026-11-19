import { describe, expect, it } from "vitest";
import { THEMES } from "./api";
import { knownTheme, themeAttribute } from "./theme";

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
