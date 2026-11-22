import { afterEach, describe, expect, it } from "vitest";
import { en } from "../locales/en";
import { fr } from "../locales/fr";
import { formatNumber, knownLang, knownLanguageSetting, lang, langOf, live, locale, plural, setLang, t } from "./i18n";

/** Chemins de toutes les clés d'un dictionnaire, avec le type de chaque valeur (et l'arité des fonctions). */
function shape(value: unknown, path = ""): string[] {
  if (typeof value === "function") return [`${path}()${value.length}`];
  if (typeof value === "string") return [path];
  if (Array.isArray(value)) return [`${path}[${value.length}]`, ...value.flatMap((v, i) => shape(v, `${path}[${i}]`))];
  if (value && typeof value === "object") {
    return Object.keys(value)
      .sort()
      .flatMap((k) => shape((value as Record<string, unknown>)[k], `${path}.${k}`));
  }
  return [`${path}:${typeof value}`];
}

/** Toutes les chaînes et le source de toutes les fonctions d'un dictionnaire. */
function texts(value: unknown): string[] {
  if (typeof value === "function") return [value.toString()];
  if (typeof value === "string") return [value];
  if (value && typeof value === "object") return Object.values(value).flatMap(texts);
  return [];
}

afterEach(() => setLang("fr"));

describe("dictionnaires", () => {
  it("le français et l'anglais ont les mêmes clés, les mêmes types et les mêmes signatures", () => {
    expect(shape(en)).toEqual(shape(fr));
  });

  it("aucun tiret cadratin, ni en français ni en anglais", () => {
    for (const dict of [fr, en]) {
      expect(texts(dict).filter((s) => s.includes("—"))).toEqual([]);
    }
  });

  it("aucune chaîne anglaise vide", () => {
    expect(texts(en).filter((s) => s.trim() === "")).toEqual([]);
  });
});

describe("langue", () => {
  it("français par défaut, anglais à la demande", () => {
    expect(lang()).toBe("fr");
    expect(locale()).toBe("fr-FR");
    expect(t.common.status.todo).toBe("À faire");
    setLang("en");
    expect(lang()).toBe("en");
    expect(locale()).toBe("en-US");
    expect(t.common.status.todo).toBe("Backlog");
  });

  it("valeurs relues", () => {
    expect(knownLang("en")).toBe("en");
    expect(knownLang("de")).toBe("fr");
    expect(knownLang(null)).toBe("fr");
    expect(knownLanguageSetting("en")).toBe("en");
    expect(knownLanguageSetting("klingon")).toBe("system");
    expect(langOf("fr-CA")).toBe("fr");
    expect(langOf("en-GB")).toBe("en");
    expect(langOf("de-DE")).toBe("en");
    expect(langOf(undefined)).toBe("en");
  });

  it("nombres et accords dans la langue actuelle", () => {
    expect(formatNumber(1240)).toBe("1 240");
    expect(plural(0, "jeu", "jeux")).toBe("jeu");
    expect(plural(2, "jeu", "jeux")).toBe("jeux");
    setLang("en");
    expect(formatNumber(1240)).toBe("1,240");
    expect(plural(0, "game", "games")).toBe("games");
    expect(plural(1, "game", "games")).toBe("game");
  });

  it("une table vivante suit la langue", () => {
    const labels = live(() => t.common.status);
    expect(labels.done).toBe("Terminé");
    setLang("en");
    expect(labels.done).toBe("Finished");
    expect(Object.keys(labels)).toEqual(["todo", "playing", "done", "dropped"]);
    expect(Object.entries(labels)[0]).toEqual(["todo", "Backlog"]);
  });
});
