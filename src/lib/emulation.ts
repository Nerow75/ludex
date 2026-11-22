// Émulation (`library/emulation` côté Rust) : textes et calculs purs de la section des Paramètres
// et de la fiche.
import type { EmulationInfo, EmuSystem, Game } from "./api";
import { t } from "./i18n";

/** Dossier de ROMs montré dans les Paramètres. */
export interface FolderRow {
  path: string;
  /** « Ajouté par toi », « Connu de RetroArch »… */
  origin: string;
  found: boolean;
  /** Dossier choisi par l'utilisateur : il peut le retirer. */
  removable: boolean;
}

/** « 6 émulateurs trouvés : Ryujinx, Eden, melonDS, mGBA, RetroArch » (noms sans doublon). */
export function emulatorSummary(info: EmulationInfo): string {
  const tx = t.settings.emulation;
  if (info.emulators.length === 0) return tx.noEmulator;
  const names = [...new Set(info.emulators.map((e) => e.name))];
  return tx.emulators(info.emulators.length, names.join(", "));
}

/** Dossiers parcourus, ceux choisis par l'utilisateur d'abord ; un dossier choisi pas encore lu
 * (scan en cours) est montré quand même. */
export function folderRows(info: EmulationInfo): FolderRow[] {
  const tx = t.settings.emulation;
  const key = (p: string) => p.replace(/[\\/]+$/, "").replace(/\//g, "\\").toLowerCase();
  const mine = new Set(info.romDirs.map(key));
  const rows: FolderRow[] = info.romDirs.map((path) => {
    const seen = info.folders.find((f) => key(f.path) === key(path));
    return { path, origin: tx.originUser, found: seen?.found ?? true, removable: true };
  });
  for (const f of info.folders) {
    if (mine.has(key(f.path))) continue;
    const origin = f.origin === "emulator" && f.emulator ? tx.originEmulator(f.emulator) : f.origin === "user" ? tx.originUser : tx.originFolder;
    rows.push({ path: f.path, origin, found: f.found, removable: false });
  }
  // Dossiers lus avant les absents (disque débranché, dossier d'un ancien réglage).
  return rows.sort((a, b) => Number(b.removable) - Number(a.removable) || Number(b.found) - Number(a.found));
}

/** Consoles où choisir l'émulateur : au moins deux émulateurs installés, celles qui ont des jeux d'abord. */
export function systemRows(info: EmulationInfo): EmuSystem[] {
  return info.systems
    .filter((s) => s.emulators.length >= 2)
    .sort((a, b) => b.games - a.games || a.name.localeCompare(b.name));
}

/** Console et émulateur d'un jeu, pour l'en-tête de la fiche : « Nintendo Switch · Ryujinx ». */
export function emulationLine(game: Pick<Game, "emulation">): string | null {
  const e = game.emulation;
  return e ? `${e.systemName} · ${e.emulator}` : null;
}
