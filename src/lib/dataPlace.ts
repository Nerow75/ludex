// Emplacement des données lourdes de Ludex (écran d'accueil et Paramètres) : fonctions pures.
import type { DataLocation, DiskChoice } from "./api";

/** Place laissée libre en plus des données (comme `relocate::MARGIN`). */
export const MARGIN = 64 * 1024 * 1024;

/** « 0 Ko », « 512 Ko », « 6,3 Mo », « 19 Mo », « 1,2 Go », « 2 To » (unités de 1 000). */
export function bytesLabel(bytes: number): string {
  const units: [number, string][] = [
    [1e12, "To"],
    [1e9, "Go"],
    [1e6, "Mo"],
  ];
  const [size, unit] = units.find(([n]) => bytes >= n) ?? [1e3, "Ko"];
  const value = bytes / size;
  const text = value < 10 ? value.toFixed(1).replace(".", ",").replace(/,0$/, "") : String(Math.round(value));
  return `${text}\u00a0${unit}`;
}

/** Octets qui bougeraient : le cache, plus les copies si leur dossier suit. */
export const heavyBytes = (loc: DataLocation): number => loc.cacheBytes + (loc.copiesSeparate ? 0 : loc.copiesBytes);

export interface DiskOption {
  disk: DiskChoice;
  /** « D: Jeux », ou « D: ». */
  label: string;
  /** « 812 Go libres sur 1 To ». */
  detail: string;
  /** Mentions : « Windows », « Actuel », « Amovible ». */
  tags: string[];
  /** Part occupée, 0 à 1. */
  used: number;
  /** Moins de 10 % libres. */
  low: boolean;
  /** Assez de place pour les données et la marge. */
  fits: boolean;
  /** Choix possible : pas le disque actuel, assez de place. */
  selectable: boolean;
}

/** Disques proposés : celui de Windows en premier, puis par lettre. */
export function diskOptions(loc: DataLocation): DiskOption[] {
  const need = heavyBytes(loc) + MARGIN;
  return [...loc.disks]
    .sort((a, b) => Number(b.system) - Number(a.system) || a.letter.localeCompare(b.letter, "fr"))
    .map((disk) => {
      const fits = disk.free >= need;
      const tags = [disk.system && "Windows", disk.current && "Actuel", disk.removable && "Amovible"].filter(Boolean) as string[];
      return {
        disk,
        label: disk.name ? `${disk.letter} ${disk.name}` : disk.letter,
        detail: `${bytesLabel(disk.free)} libres sur ${bytesLabel(disk.total)}`,
        tags,
        used: disk.total > 0 ? Math.min(1, Math.max(0, 1 - disk.free / disk.total)) : 0,
        low: disk.total > 0 && disk.free / disk.total < 0.1,
        fits,
        selectable: fits && !disk.current,
      };
    });
}

/** Dossiers où iraient le cache et les copies pour un dossier racine (« D:\Ludex »). */
export function targetPlaces(root: string, loc: DataLocation): { cache: string; copies: string } {
  const base = root.replace(/[\\/]+$/, "");
  return { cache: `${base}\\Cache`, copies: loc.copiesSeparate ? loc.copiesDir : `${base}\\Sauvegardes` };
}

/** Phrase de l'emplacement actuel. */
export function currentText(loc: DataLocation): string {
  if (loc.missing) return `${loc.root} est introuvable (disque débranché ?) : images et sauvegardes attendent son retour.`;
  if (!loc.root) return "Emplacement d'origine, sur le disque de Windows.";
  return `Dans ${loc.root}.`;
}

/** Ce qui sera déplacé, en une phrase. */
export function moveText(loc: DataLocation, root: string): string {
  const places = targetPlaces(root, loc);
  const copies = loc.copiesSeparate
    ? "Les copies de sauvegardes restent dans le dossier choisi à part."
    : `Copies de sauvegardes dans ${places.copies}.`;
  return `${bytesLabel(heavyBytes(loc))} à déplacer. Cache dans ${places.cache}. ${copies}`;
}
