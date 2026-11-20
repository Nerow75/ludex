// Page « Captures » : dossiers (un par jeu), tri, filtre images / vidéos et regroupement par mois.
import type { Screenshot, ScreenshotGroup } from "./api";

/** Dossier de la liste de gauche : un jeu, un appid Steam désinstallé, ou les captures sans jeu. */
export interface ShotFolder {
  /** Clé stable : id du jeu, `appid:<n>` ou `autres`. */
  key: string;
  gameId: string | null;
  title: string;
  /** Précision sous le titre (« Jeu désinstallé »), `null` : aucune. */
  note: string | null;
  /** La plus récente d'abord. */
  shots: Screenshot[];
}

export type ShotSort = "recent" | "title" | "count";
export type ShotKind = "all" | "images" | "videos";

export const SHOT_SORTS: readonly { id: ShotSort; label: string }[] = [
  { id: "recent", label: "Récentes" },
  { id: "title", label: "A → Z" },
  { id: "count", label: "Nombre" },
];
export const SHOT_KINDS: readonly { id: ShotKind; label: string }[] = [
  { id: "all", label: "Tout" },
  { id: "images", label: "Images" },
  { id: "videos", label: "Vidéos" },
];

const recentFirst = (a: Screenshot, b: Screenshot) => b.taken - a.taken || a.key.localeCompare(b.key);
const TITLE_ORDER = new Intl.Collator("fr", { sensitivity: "base", numeric: true });

/**
 * Dossiers de la page à partir des groupes de Rust. « Autres captures » est éclaté : une entrée par
 * appid Steam (jeu désinstallé, souvent l'essentiel des captures), le reste (Game Bar sans jeu) garde
 * le nom « Autres captures ». Chaque dossier est trié, la plus récente d'abord ; les vides sont écartés.
 */
export function shotFolders(groups: readonly ScreenshotGroup[]): ShotFolder[] {
  const folders: ShotFolder[] = [];
  const byAppid = new Map<number, Screenshot[]>();
  const loose: Screenshot[] = [];
  for (const g of groups) {
    if (g.gameId) {
      folders.push({ key: g.gameId, gameId: g.gameId, title: g.title, note: null, shots: [...g.shots].sort(recentFirst) });
      continue;
    }
    for (const s of g.shots) {
      if (s.steamAppid) byAppid.set(s.steamAppid, [...(byAppid.get(s.steamAppid) ?? []), s]);
      else loose.push(s);
    }
  }
  for (const [appid, shots] of byAppid) {
    folders.push({ key: `appid:${appid}`, gameId: null, title: `Jeu Steam ${appid}`, note: "Désinstallé", shots: shots.sort(recentFirst) });
  }
  if (loose.length) folders.push({ key: "autres", gameId: null, title: "Autres captures", note: null, shots: loose.sort(recentFirst) });
  return folders.filter((f) => f.shots.length > 0);
}

export const isKind = (shot: Screenshot, kind: ShotKind) => kind === "all" || shot.video === (kind === "videos");

/** Dossiers restreints à un genre de capture (dossiers vides écartés), puis triés. */
export function arrangeFolders(folders: readonly ShotFolder[], sort: ShotSort, kind: ShotKind): ShotFolder[] {
  const kept = folders
    .map((f) => (kind === "all" ? f : { ...f, shots: f.shots.filter((s) => isKind(s, kind)) }))
    .filter((f) => f.shots.length > 0);
  const byTitle = (a: ShotFolder, b: ShotFolder) => TITLE_ORDER.compare(a.title, b.title) || a.key.localeCompare(b.key);
  // « Autres captures » (sans jeu) toujours en dernier : ce n'est pas un jeu.
  const last = (f: ShotFolder) => (f.key === "autres" ? 1 : 0);
  return kept.sort(
    (a, b) =>
      last(a) - last(b) ||
      (sort === "recent" ? b.shots[0].taken - a.shots[0].taken : 0) ||
      (sort === "count" ? b.shots.length - a.shots.length : 0) ||
      byTitle(a, b),
  );
}

/** Toutes les captures des dossiers, la plus récente d'abord. */
export const allShots = (folders: readonly ShotFolder[]): Screenshot[] => folders.flatMap((f) => f.shots).sort(recentFirst);

export interface ShotMonth {
  /** « 2026-11 ». */
  key: string;
  /** « Novembre 2026 ». */
  label: string;
  shots: Screenshot[];
  /** Position de la première capture du mois dans la liste d'origine (pour la visionneuse). */
  start: number;
}

const monthName = new Intl.DateTimeFormat("fr-FR", { month: "long", year: "numeric" });

/** Captures (déjà triées) rangées par mois, en heure locale, dans l'ordre reçu. */
export function byMonth(shots: readonly Screenshot[]): ShotMonth[] {
  const months: ShotMonth[] = [];
  shots.forEach((s, i) => {
    const d = new Date(s.taken * 1000);
    const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
    const last = months[months.length - 1];
    if (last?.key === key) last.shots.push(s);
    else {
      const label = monthName.format(d);
      months.push({ key, label: label.charAt(0).toUpperCase() + label.slice(1), shots: [s], start: i });
    }
  });
  return months;
}

/** Nombre de captures de chaque genre, pour savoir s'il faut proposer le filtre. */
export function kindCounts(folders: readonly ShotFolder[]): Record<ShotKind, number> {
  let videos = 0;
  let all = 0;
  for (const f of folders) {
    all += f.shots.length;
    for (const s of f.shots) if (s.video) videos += 1;
  }
  return { all, images: all - videos, videos };
}

/** « 1 capture », « 12 captures », « 3 vidéos », selon le filtre. */
export function shotCount(n: number, kind: ShotKind = "all"): string {
  const word = kind === "videos" ? "vidéo" : kind === "images" ? "image" : "capture";
  return `${n} ${word}${n > 1 ? "s" : ""}`;
}
