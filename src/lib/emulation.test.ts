import { afterEach, describe, expect, it } from "vitest";
import type { EmulationInfo } from "./api";
import { emulationLine, emulatorSummary, folderRows, systemRows } from "./emulation";
import { setLang } from "./i18n";

afterEach(() => setLang("fr"));

const infos = (champs: Partial<EmulationInfo> = {}): EmulationInfo => ({
  scanned: true,
  emulators: [
    { key: "ryujinx", name: "Ryujinx", path: "E:\\Jeux\\EmuSwitch\\Ryujinx.exe", systems: ["switch"] },
    { key: "ryujinx", name: "Ryujinx", path: "D:\\Logiciel SSD\\RyujinxCanary\\Ryujinx.exe", systems: ["switch"] },
    { key: "mgba", name: "mGBA", path: "D:\\Logiciel SSD\\mGBA\\mGBA.exe", systems: ["gba", "gb", "gbc"] },
    { key: "retroarch", name: "RetroArch", path: "D:\\Logiciel SSD\\RetroArch\\retroarch.exe", systems: ["gb", "gbc", "gba"] },
  ],
  systems: [
    { id: "gb", name: "Game Boy", games: 0, emulators: [{ key: "mgba", name: "mGBA" }, { key: "retroarch", name: "RetroArch" }], chosen: null },
    { id: "switch", name: "Nintendo Switch", games: 4, emulators: [{ key: "ryujinx", name: "Ryujinx" }], chosen: null },
    { id: "gba", name: "Game Boy Advance", games: 2, emulators: [{ key: "mgba", name: "mGBA" }, { key: "retroarch", name: "RetroArch" }], chosen: "mgba" },
  ],
  folders: [
    { path: "E:\\Jeux\\EmuSwitch\\games", origin: "emulator", emulator: "Ryujinx", found: true },
    { path: "Z:\\ROMs", origin: "emulator", emulator: "RetroArch", found: false },
    { path: "E:\\Jeux Switch", origin: "folder", emulator: null, found: true },
    { path: "D:\\ROMs\\GBA", origin: "user", emulator: null, found: true },
  ],
  romDirs: ["D:\\ROMs\\GBA", "F:\\Retro"],
  ...champs,
});

describe("émulation", () => {
  it("résume les émulateurs trouvés, sans doublon de nom", () => {
    expect(emulatorSummary(infos())).toBe("4 émulateurs trouvés : Ryujinx, mGBA, RetroArch");
    expect(emulatorSummary(infos({ emulators: infos().emulators.slice(2, 3) }))).toBe("1 émulateur trouvé : mGBA");
    expect(emulatorSummary(infos({ emulators: [] }))).toMatch(/^Aucun émulateur trouvé/);
    setLang("en");
    expect(emulatorSummary(infos())).toBe("4 emulators found: Ryujinx, mGBA, RetroArch");
  });

  it("montre les dossiers choisis d'abord, puis les lus, puis les absents", () => {
    const rows = folderRows(infos());
    expect(rows.map((r) => [r.path, r.origin, r.found, r.removable])).toEqual([
      ["D:\\ROMs\\GBA", "Ajouté par toi", true, true],
      // Choisi, pas encore lu par un scan : montré quand même.
      ["F:\\Retro", "Ajouté par toi", true, true],
      ["E:\\Jeux\\EmuSwitch\\games", "Connu de Ryujinx", true, false],
      ["E:\\Jeux Switch", "Nommé d'après une console", true, false],
      ["Z:\\ROMs", "Connu de RetroArch", false, false],
    ]);
    // Même dossier écrit autrement (casse, barre finale, `/`) : une seule ligne.
    const autre = folderRows(infos({ romDirs: ["d:/roms/gba/"] }));
    expect(autre.filter((r) => r.path.toLowerCase().includes("gba"))).toHaveLength(1);
    setLang("en");
    expect(folderRows(infos())[2].origin).toBe("Known to Ryujinx");
  });

  it("ne propose un choix que s'il y a deux émulateurs, consoles avec des jeux d'abord", () => {
    expect(systemRows(infos()).map((s) => s.id)).toEqual(["gba", "gb"]);
    expect(systemRows(infos({ systems: [] }))).toEqual([]);
  });

  it("écrit la console et l'émulateur d'un jeu", () => {
    expect(emulationLine({ emulation: { system: "switch", systemName: "Nintendo Switch", emulator: "Ryujinx", emulatorKey: "ryujinx" } })).toBe("Nintendo Switch · Ryujinx");
    expect(emulationLine({ emulation: null })).toBeNull();
  });
});
