import { afterEach, describe, expect, it } from "vitest";
import { setLang } from "./i18n";
import type { WorkshopMod } from "./api";
import { modDetail, modsSummary, modTitle } from "./workshop";

const mod = (fields: Partial<WorkshopMod> = {}): WorkshopMod => ({
  id: "2824688072",
  title: "Calamity Mod",
  previewUrl: null,
  size: 404_391_973,
  updated: 1786328541,
  pending: false,
  present: true,
  ...fields,
});

describe("modTitle", () => {
  it("prend le titre, sinon l'id Workshop", () => {
    expect(modTitle(mod())).toBe("Calamity Mod");
    expect(modTitle(mod({ title: null }))).toBe("Mod 2824688072");
  });
});

describe("modsSummary", () => {
  it("compte les mods, la taille et les mises à jour en attente", () => {
    expect(modsSummary({ mods: [mod()], total: 404_391_973, folder: true, stale: false })).toBe("1 mod installé, 404,4 Mo");
    const list = [mod(), mod({ id: "1", pending: true }), mod({ id: "2", pending: true })];
    expect(modsSummary({ mods: list, total: 3_300_000_000, folder: true, stale: false })).toBe("3 mods installés, 3,3 Go, dont 2 à mettre à jour");
  });
});

describe("modDetail", () => {
  it("donne la taille et la date de la version installée", () => {
    expect(modDetail(mod())).toBe("404,4 Mo · mis à jour le 10 août 2026");
    expect(modDetail(mod({ updated: null, size: 46_150 }))).toBe("46 Ko");
  });

  it("signale un dossier absent", () => {
    expect(modDetail(mod({ present: false, updated: null }))).toBe("Dossier absent");
  });
});

describe("textes des mods en anglais", () => {
  afterEach(() => setLang("fr"));

  it("titre, résumé et ligne d'un mod", () => {
    setLang("en");
    expect(modTitle(mod({ title: null }))).toBe("Mod 2824688072");
    // La taille vient de `formatBytes` (format.ts) : seule la phrase de Ludex est vérifiée ici.
    expect(modsSummary({ mods: [mod()], total: 404_391_973, folder: true, stale: false })).toMatch(/^1 mod installed, \S/);
    const list = [mod(), mod({ id: "1", pending: true }), mod({ id: "2", pending: true })];
    expect(modsSummary({ mods: list, total: 3_300_000_000, folder: true, stale: false })).toMatch(/^3 mods installed, .+, 2 need an update$/);
    expect(modsSummary({ mods: [mod({ pending: true })], total: 1, folder: true, stale: false })).toMatch(/, 1 needs an update$/);
    expect(modDetail(mod({ present: false, updated: null }))).toBe("Folder missing");
    expect(modDetail(mod())).toMatch(/ · updated /);
  });
});
