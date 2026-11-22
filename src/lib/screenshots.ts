// Page « Captures » : dossiers (un par jeu), tri, filtre images / vidéos et regroupement par mois.
import type { Screenshot, ScreenshotGroup } from "./api";
import { live, locale, t } from "./i18n";

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

/** Libellés relus dans la langue actuelle à chaque accès (`live`). */
export const SHOT_SORTS: readonly { id: ShotSort; label: string }[] = live(() => [
  { id: "recent", label: t.extras.shots.sorts.recent },
  { id: "title", label: t.extras.shots.sorts.title },
  { id: "count", label: t.extras.shots.sorts.count },
]);
export const SHOT_KINDS: readonly { id: ShotKind; label: string }[] = live(() => [
  { id: "all", label: t.extras.shots.kinds.all },
  { id: "images", label: t.extras.shots.kinds.images },
  { id: "videos", label: t.extras.shots.kinds.videos },
]);

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
    folders.push({ key: `appid:${appid}`, gameId: null, title: t.extras.shots.steamGame(appid), note: t.extras.shots.uninstalled, shots: shots.sort(recentFirst) });
  }
  if (loose.length) folders.push({ key: "autres", gameId: null, title: t.extras.shots.others, note: null, shots: loose.sort(recentFirst) });
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

const monthNames = new Map<string, Intl.DateTimeFormat>();
/** « novembre 2026 », « November 2026 » : format gardé par locale. */
function monthName(d: Date): string {
  const loc = locale();
  let f = monthNames.get(loc);
  if (!f) {
    f = new Intl.DateTimeFormat(loc, { month: "long", year: "numeric" });
    monthNames.set(loc, f);
  }
  return f.format(d);
}

/** Captures (déjà triées) rangées par mois, en heure locale, dans l'ordre reçu. */
export function byMonth(shots: readonly Screenshot[]): ShotMonth[] {
  const months: ShotMonth[] = [];
  shots.forEach((s, i) => {
    const d = new Date(s.taken * 1000);
    const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
    const last = months[months.length - 1];
    if (last?.key === key) last.shots.push(s);
    else {
      const label = monthName(d);
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
  return t.extras.shots.count(n, kind);
}
