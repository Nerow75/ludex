import { afterEach, describe, expect, it } from "vitest";
import { storeName, withStore } from "./achievements";
import { setLang } from "./i18n";

describe("boutique des succès", () => {
  afterEach(() => setLang("fr"));

  it("nomme la boutique", () => {
    expect(storeName("steam")).toBe("Steam");
    expect(storeName("gog")).toBe("GOG");
  });

  it("ajoute la boutique après le texte", () => {
    expect(withStore("The Witcher", "gog")).toBe("The Witcher · GOG");
    setLang("en");
    expect(withStore("Portal 2", "steam")).toBe("Portal 2 · Steam");
  });
});
