import { afterEach, describe, expect, it } from "vitest";
import type { MoveProgress, MoveTarget } from "./api";
import { firstUsable, launcherHelp, moveActive, movePercent, moveStatus, targetNote, targetUsable } from "./gameMove";
import { setLang } from "./i18n";
import { espaces } from "../test/jeu";

const cible = (fields: Partial<MoveTarget> = {}): MoveTarget => ({
  key: "D:\\Jeux",
  path: "D:\\Jeux\\Frostpunk",
  letter: "D:",
  name: "SSD",
  free: 120_000_000_000,
  total: 1_000_000_000_000,
  create: false,
  sameDrive: false,
  enough: true,
  taken: false,
  ...fields,
});

const job = (fields: Partial<MoveProgress> = {}): MoveProgress => ({
  id: "other:e-jeux-frostpunk",
  title: "Frostpunk",
  phase: "copying",
  copied: 2_500_000_000,
  total: 10_000_000_000,
  speed: null,
  remaining: null,
  from: "E:\\Jeux\\Frostpunk",
  to: "D:\\Jeux\\Frostpunk",
  error: null,
  warnings: [],
  ...fields,
});

describe("destinations", () => {
  it("refus, place et dossier créé", () => {
    expect(targetUsable(cible())).toBe(true);
    expect(targetUsable(cible({ enough: false }))).toBe(false);
    expect(targetUsable(cible({ taken: true }))).toBe(false);
    expect(espaces(targetNote(cible()))).toBe("120,0 Go libres");
    expect(targetNote(cible({ sameDrive: true }))).toBe("Même disque : instantané");
    expect(espaces(targetNote(cible({ create: true })))).toBe("120,0 Go libres · D:\\Jeux sera créé");
    expect(espaces(targetNote(cible({ enough: false, free: 3_000_000_000 })))).toBe("Pas assez de place · 3,0 Go libres");
    expect(targetNote(cible({ taken: true, enough: false }))).toBe("Un dossier du même nom existe déjà");
    expect(firstUsable([cible({ taken: true }), cible({ key: "E:\\Games" })])?.key).toBe("E:\\Games");
    expect(firstUsable([cible({ enough: false })])).toBeNull();
  });

  it("explication par launcher", () => {
    expect(launcherHelp("epic")).toContain("Epic Games garde l'emplacement");
    expect(launcherHelp("ubisoft")).toContain("Localiser le jeu installé");
    expect(launcherHelp("battlenet")).toContain("Localiser ce jeu");
    expect(launcherHelp("itch")).toContain("« Gérer » puis « Déplacer »");
  });
});

describe("progression", () => {
  it("phases actives et pourcentage", () => {
    expect(moveActive(null)).toBe(false);
    expect(moveActive(job())).toBe(true);
    expect(moveActive(job({ phase: "ready" }))).toBe(true);
    expect(moveActive(job({ phase: "done" }))).toBe(false);
    expect(moveActive(job({ phase: "failed" }))).toBe(false);
    expect(movePercent(job())).toBe(25);
    expect(movePercent(job({ phase: "preparing" }))).toBeNull();
    expect(movePercent(job({ total: 0 }))).toBeNull();
    expect(movePercent(job({ phase: "ready" }))).toBe(100);
  });

  it("ligne d'état", () => {
    expect(espaces(moveStatus(job()))).toBe("Copie : 2,5 Go sur 10 Go");
    expect(espaces(moveStatus(job({ speed: 150_000_000, remaining: 50 })))).toBe("Copie : 2,5 Go sur 10 Go · 150 Mo/s · moins d'une minute");
    expect(moveStatus(job({ phase: "ready" }))).toBe("Copie vérifiée. Supprimer l'ancien dossier E:\\Jeux\\Frostpunk et finir le déplacement ?");
    expect(moveStatus(job({ phase: "done" }))).toBe("Déplacé dans D:\\Jeux\\Frostpunk.");
    expect(moveStatus(job({ phase: "cancelled" }))).toBe("Déplacement annulé : le jeu n'a pas bougé.");
    expect(moveStatus(job({ phase: "failed", error: "Disque plein pendant la copie de a.pak." }))).toBe(
      "Déplacement impossible : Disque plein pendant la copie de a.pak.",
    );
  });
});

describe("textes en anglais", () => {
  afterEach(() => setLang("fr"));

  it("destinations et progression", () => {
    setLang("en");
    expect(espaces(targetNote(cible({ create: true })))).toBe("120.0 GB free · D:\\Jeux will be created");
    expect(launcherHelp("gog")).toBe(
      "GOG keeps the game's location in its own files: Ludex doesn't move it, so as not to break GOG. Uninstall it, then reinstall it on the other drive.",
    );
    expect(espaces(moveStatus(job()))).toBe("Copying: 2.5 GB of 10 GB");
    expect(moveStatus(job({ phase: "ready" }))).toBe("Copy checked. Delete the old folder E:\\Jeux\\Frostpunk and finish the move?");
  });
});
