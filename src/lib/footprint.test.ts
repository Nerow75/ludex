import { afterEach, describe, expect, it } from "vitest";
import { setLang } from "./i18n";
import type { Footprint, FootprintFolder, FootprintUninstall } from "./api";
import { actionHint, folderSize, footprintStatus, insideLabel, installDay, mainAction, presentBytes } from "./footprint";

const folder = (fields: Partial<FootprintFolder> = {}): FootprintFolder => ({
  item: "install",
  label: "Dossier d'installation",
  kind: "install",
  path: "E:\\Jeux\\Jeu",
  exists: true,
  file: false,
  bytes: 2_000_000_000,
  inside: null,
  ...fields,
});

const entry = (fields: Partial<FootprintUninstall> = {}): FootprintUninstall => ({
  item: "uninstall-0",
  key: "HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Jeu",
  exists: true,
  name: "Jeu",
  publisher: null,
  version: null,
  declaredSize: null,
  installDate: null,
  location: null,
  command: "\"E:\\Jeux\\Jeu\\unins000.exe\"",
  action: "run",
  refusal: null,
  ...fields,
});

const fp = (fields: Partial<Footprint> = {}): Footprint => ({
  id: "other:jeu",
  title: "Jeu",
  platform: "other",
  installed: true,
  folders: [folder()],
  uninstall: [],
  keys: [],
  savesRegistry: false,
  savesNote: null,
  steam: false,
  storeApp: false,
  leftovers: 0,
  ...fields,
});

describe("folderSize", () => {
  it("dit l'état ou la taille", () => {
    expect(folderSize(folder())).toBe("2,0 Go");
    expect(folderSize(folder({ exists: false }))).toBe("Supprimé");
    expect(folderSize(folder({ bytes: null }))).toBe("Mesure en cours");
    expect(folderSize(folder({ kind: "cache", bytes: null }))).toBe("Taille inconnue");
    expect(folderSize(folder({ bytes: 0 }))).toBe("Vide");
  });
});

describe("presentBytes et footprintStatus", () => {
  it("ne compte pas deux fois ce qui est dans le dossier du jeu", () => {
    const f = fp({
      folders: [
        folder(),
        folder({ item: "save-0", kind: "saves", bytes: 10, inside: "install" }),
        folder({ item: "shaders", kind: "cache", bytes: 500_000_000 }),
        folder({ item: "save-1", kind: "saves", bytes: 3, exists: false }),
        folder({ item: "cloud", label: "Données du compte Steam", kind: "saves", bytes: 20, exists: false }),
        folder({ item: "save-2", kind: "saves", bytes: 7, inside: "cloud" }),
      ],
    });
    expect(presentBytes(f)).toEqual({ bytes: 2_500_000_007, unknown: false });
    expect(insideLabel(f, f.folders[1])).toBe("dans le dossier du jeu");
    expect(insideLabel(f, f.folders[5])).toBe("dans « Données du compte Steam »");
    expect(insideLabel(f, f.folders[0])).toBeNull();
    expect(footprintStatus(f)).toBe("Installé : 2,5 Go sur le PC, sauvegardes et caches compris");
    expect(footprintStatus(fp({ folders: [folder({ bytes: null })] }))).toBe("Installé, taille en cours de mesure");
    expect(footprintStatus(fp())).toBe("Installé : 2,0 Go sur le PC");
  });

  it("compte les restes après la désinstallation", () => {
    expect(footprintStatus(fp({ installed: false, leftovers: 0 }))).toBe("Désinstallé : plus rien aux emplacements connus de Ludex");
    expect(footprintStatus(fp({ installed: false, leftovers: 1 }))).toBe("Désinstallé : 1 reste encore sur le PC");
    expect(footprintStatus(fp({ installed: false, leftovers: 3 }))).toBe("Désinstallé : 3 restes encore sur le PC");
  });
});

describe("mainAction", () => {
  it("préfère Steam, puis le désinstalleur, puis les Paramètres, puis le dossier", () => {
    expect(mainAction(fp({ steam: true, uninstall: [entry()] }))).toEqual({ kind: "steam" });
    expect(mainAction(fp({ uninstall: [entry({ action: "none" }), entry({ item: "uninstall-1" })] }))).toEqual({ kind: "run", item: "uninstall-1" });
    expect(mainAction(fp({ uninstall: [entry({ exists: false })], storeApp: true }))).toEqual({ kind: "settings" });
    expect(mainAction(fp())).toEqual({ kind: "folder" });
    expect(mainAction(fp({ folders: [folder({ exists: false })] }))).toBeNull();
    expect(mainAction(fp({ installed: false, steam: true }))).toBeNull();
  });

  it("explique chaque bouton", () => {
    expect(actionHint(fp(), { kind: "folder" })).toBe("Aucun désinstalleur trouvé pour ce jeu : supprime son dossier à la main.");
    expect(actionHint(fp({ platform: "epic" }), { kind: "folder" })).toContain("depuis Epic");
    expect(actionHint(fp({ installed: false }), null)).toBe("");
    expect(installDay("2024-01-15")).toBe("15/01/2024");
    expect(installDay(null)).toBeNull();
  });
});

describe("textes en anglais", () => {
  afterEach(() => setLang("fr"));

  it("états, emplacements et explications", () => {
    setLang("en");
    expect(folderSize(folder({ exists: false }))).toBe("Deleted");
    expect(folderSize(folder({ bytes: null }))).toBe("Measuring");
    expect(folderSize(folder({ kind: "cache", bytes: null }))).toBe("Unknown size");
    expect(folderSize(folder({ bytes: 0 }))).toBe("Empty");
    const f = fp({ folders: [folder(), folder({ item: "save-0", kind: "saves", bytes: 10, inside: "install" })] });
    expect(insideLabel(f, f.folders[1])).toBe("in the game folder");
    expect(footprintStatus(fp({ folders: [folder({ bytes: null })] }))).toBe("Installed, size being measured");
    // La taille vient de `formatSize` (format.ts) : seule la phrase de Ludex est vérifiée ici.
    expect(footprintStatus(f)).toMatch(/^Installed: .+ on this PC, saves and caches included$/);
    expect(footprintStatus(fp({ folders: [folder(), folder({ item: "x", kind: "cache", bytes: null })] }))).toMatch(/^Installed: at least /);
    expect(footprintStatus(fp({ installed: false, leftovers: 0 }))).toBe("Uninstalled: nothing left in the places Ludex knows");
    expect(footprintStatus(fp({ installed: false, leftovers: 1 }))).toBe("Uninstalled: 1 leftover still on this PC");
    expect(footprintStatus(fp({ installed: false, leftovers: 3 }))).toBe("Uninstalled: 3 leftovers still on this PC");
    expect(actionHint(fp(), { kind: "folder" })).toBe("No uninstaller found for this game: delete its folder by hand.");
    expect(actionHint(fp({ platform: "epic" }), { kind: "folder" })).toBe(
      "No uninstaller found: uninstall it from Epic Games, or delete its folder by hand.",
    );
    expect(actionHint(fp(), null)).toBe("Game folder not found.");
    expect(installDay("2024-01-15")).toBe("01/15/2024");
  });
});
