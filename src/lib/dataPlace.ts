// Emplacement des données lourdes de Ludex (écran d'accueil et Paramètres) : fonctions pures.
import type { DataLocation, DiskChoice } from "./api";
import { lang, t } from "./i18n";

/** Place laissée libre en plus des données (comme `relocate::MARGIN`). */
export const MARGIN = 64 * 1024 * 1024;

/** « 0 Ko », « 512 Ko », « 6,3 Mo », « 19 Mo », « 1,2 Go », « 2 To » (unités de 1 000) ; « 6.3 MB » en anglais. */
export function bytesLabel(bytes: number): string {
  const u = t.settings.place.units;
  const units: [number, string][] = [
    [1e12, u.tb],
    [1e9, u.gb],
    [1e6, u.mb],
  ];
  const [size, unit] = units.find(([n]) => bytes >= n) ?? [1e3, u.kb];
  const value = bytes / size;
  const fixed = value.toFixed(1).replace(/\.0$/, "");
  const text = value < 10 ? (lang() === "fr" ? fixed.replace(".", ",") : fixed) : String(Math.round(value));
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
      const tag = t.settings.place.tags;
      const tags = [disk.system && tag.system, disk.current && tag.current, disk.removable && tag.removable].filter(Boolean) as string[];
      return {
        disk,
        label: disk.name ? `${disk.letter} ${disk.name}` : disk.letter,
        detail: t.settings.place.free(bytesLabel(disk.free), bytesLabel(disk.total)),
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
  const p = t.settings.place;
  if (loc.missing) return p.missing(loc.root);
  if (!loc.root) return p.original;
  return p.inRoot(loc.root);
}

/** Ce qui sera déplacé, en une phrase. */
export function moveText(loc: DataLocation, root: string): string {
  const p = t.settings.place;
  const places = targetPlaces(root, loc);
  const copies = loc.copiesSeparate ? p.copiesKept : p.copiesIn(places.copies);
  return `${p.move(bytesLabel(heavyBytes(loc)), places.cache)} ${copies}`;
}

/** Retour à l'emplacement d'origine, en une phrase (les copies choisies à part ne bougent pas). */
export function backText(loc: DataLocation): string {
  const p = t.settings.place;
  const move = p.move(bytesLabel(heavyBytes(loc)), loc.defaultCache);
  return loc.copiesSeparate ? move : `${move} ${p.copiesIn(loc.defaultCopies)}`;
}
