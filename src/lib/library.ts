import { MOODS, STATUSES, type Game, type Mood, type Platform, type Status } from "./api";
import { cleanTitle, formatAchievements, formatPlaytime, formatSize, relativeTime, releaseYear } from "./format";
import { live, locale, t } from "./i18n";
import { prepareQuery, searchRank } from "./search";

export type Filter = "all" | "favorites" | "hidden" | Platform;
export type Sort = "recent" | "playtime" | "title" | "size" | "year" | "achievements" | "lifespan";

/** Filtres en plus du launcher : un tag Steam, une ambiance, un genre. `null` : pas de filtre. */
export interface Facets {
  tag: string | null;
  mood: Mood | null;
  genre: string | null;
}
export const NO_FACETS: Facets = { tag: null, mood: null, genre: null };

/** Libellés des tris, dans la langue actuelle (relus à chaque accès). */
export const SORT_LABEL: Record<Sort, string> = live(() => t.common.sort);
export const SORTS: readonly Sort[] = ["recent", "playtime", "title", "size", "year", "achievements", "lifespan"];

/** Bibliothèque en grille de jaquettes ou en liste dense. */
export type Layout = "grid" | "list";
export const LAYOUTS: readonly Layout[] = ["grid", "list"];

// Un seul collateur par langue : `localeCompare` avec des options en recrée un à chaque comparaison
// (des centaines de millisecondes pour trier quelques milliers de jeux). Même ordre.
const collators = new Map<string, Intl.Collator>();
/** Collateur de la langue actuelle, sans accents ni casse (tri des titres, des tags…). */
export function titleOrder(): Intl.Collator {
  const key = locale();
  let c = collators.get(key);
  if (!c) {
    c = new Intl.Collator(key, { sensitivity: "base" });
    collators.set(key, c);
  }
  return c;
}
const byTitle = (a: Game, b: Game) => titleOrder().compare(cleanTitle(a.title), cleanTitle(b.title));

/** Compare deux valeurs : celle qui manque (`null`) va toujours en fin de liste. */
function known(a: number | null, b: number | null, desc: boolean): number {
  if (a === null || b === null) return a === b ? 0 : a === null ? 1 : -1;
  return desc ? b - a : a - b;
}

/** Part des succès débloqués (0 à 1), `null` si le jeu n'a pas de succès connus. */
const achievementRatio = (g: Game) => (g.achievements && g.achievements.total > 0 ? g.achievements.unlocked / g.achievements.total : null);

/** Succès débloqués en % entier, arrondi vers le bas (99,6 % reste 99 %) ; `null` sans succès. */
export function achievementPercent(game: Game): number | null {
  const a = game.achievements;
  return a && a.total > 0 ? Math.floor((Math.min(a.unlocked, a.total) * 100) / a.total) : null;
}

/** Durée de l'histoire principale (HowLongToBeat, minutes), `null` si inconnue. */
export const storyMinutes = (g: Game) => g.hltb?.main ?? null;

const COMPARE: Record<Sort, (a: Game, b: Game) => number> = {
  recent: (a, b) => (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0) || byTitle(a, b),
  // Jamais joué (0) avant inconnu (null).
  playtime: (a, b) => (b.playtimeMinutes ?? -1) - (a.playtimeMinutes ?? -1) || byTitle(a, b),
  title: byTitle,
  size: (a, b) => (b.sizeOnDisk ?? 0) - (a.sizeOnDisk ?? 0) || byTitle(a, b),
  // Le plus récent d'abord.
  year: (a, b) => known(a.details?.releaseDate ?? null, b.details?.releaseDate ?? null, true) || byTitle(a, b),
  // Le plus avancé d'abord ; à égalité, le plus de succès débloqués.
  achievements: (a, b) =>
    known(achievementRatio(a), achievementRatio(b), true) || (b.achievements?.unlocked ?? 0) - (a.achievements?.unlocked ?? 0) || byTitle(a, b),
  // Le plus court d'abord : de quoi choisir un jeu à finir.
  lifespan: (a, b) => known(storyMinutes(a), storyMinutes(b), false) || byTitle(a, b),
};

/** Information sous le titre d'une jaquette, selon le tri choisi. */
export function sortMeta(game: Game, sort: Sort): string {
  switch (sort) {
    case "size":
      return formatSize(game.sizeOnDisk);
    case "playtime":
      return formatPlaytime(game.playtimeMinutes);
    case "year":
      return String(releaseYear(game.details?.releaseDate ?? null) ?? t.common.sortMeta.unknownYear);
    case "achievements": {
      const a = game.achievements;
      return a && a.total > 0 ? t.common.sortMeta.achievements(formatAchievements(a.unlocked, a.total)) : t.common.sortMeta.noAchievements;
    }
    case "lifespan": {
      const story = storyMinutes(game);
      return story === null ? t.common.sortMeta.unknownLifespan : t.common.sortMeta.story(formatPlaytime(story));
    }
    default:
      return relativeTime(game.lastPlayed);
  }
}

/** Filtre par statut de progression, en plus du filtre par launcher. */
export type StatusFilter = "any" | "none" | Status;
export const STATUS_FILTERS: readonly StatusFilter[] = ["any", ...STATUSES, "none"];
export const STATUS_FILTER_LABEL: Record<StatusFilter, string> = live(() => ({ ...t.common.statusFilter, ...t.common.status }));

export const matchesStatus = (game: Game, filter: StatusFilter) =>
  filter === "any" || (filter === "none" ? !game.status : game.status === filter);

export function matchesFilter(game: Game, filter: Filter): boolean {
  if (filter === "hidden") return game.hidden;
  if (game.hidden) return false;
  if (filter === "favorites") return game.favorite;
  return filter === "all" || game.platform === filter;
}

export function matchesFacets(game: Game, { tag, mood, genre }: Facets): boolean {
  return (
    (!tag || !!game.details?.tags.includes(tag)) &&
    (!mood || !!game.details?.moods.includes(mood)) &&
    (!genre || !!game.details?.genres.includes(genre))
  );
}

export const hasFacets = (f: Facets) => f.tag !== null || f.mood !== null || f.genre !== null;

/**
 * Jeux du filtre et des tags ou ambiances choisis. Pendant une recherche, classés par `searchRank`
 * (préfixe du titre, initiales, début d'un mot, sous-chaîne, fautes de frappe, puis studio,
 * éditeur ou tag), à égalité le plus récemment joué, puis le tri choisi.
 */
export function selectGames(games: Game[], filter: Filter, sort: Sort, query: string, facets: Facets = NO_FACETS): Game[] {
  const q = query.trim() ? prepareQuery(query) : null;
  const rank = new Map<string, number>();
  for (const g of games) {
    if (!matchesFilter(g, filter) || !matchesFacets(g, facets)) continue;
    const r = q ? searchRank(g, q) : 0;
    if (r !== null) rank.set(g.id, r);
  }
  return games
    .filter((g) => rank.has(g.id))
    .sort((a, b) => rank.get(a.id)! - rank.get(b.id)! || (q ? (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0) : 0) || COMPARE[sort](a, b));
}

/** Nombre de jeux d'un filtre (pastilles de la barre) : mêmes jeux que `selectGames` sans recherche, sans les trier. */
export function countGames(games: Game[], filter: Filter, facets: Facets = NO_FACETS): number {
  let n = 0;
  for (const g of games) if (matchesFilter(g, filter) && matchesFacets(g, facets)) n++;
  return n;
}

/** Les derniers jeux joués, hors jeux masqués. */
export const recentGames = (games: Game[], n: number) =>
  games.filter((g) => !g.hidden && g.lastPlayed).sort(COMPARE.recent).slice(0, n);

/** Nombre de jeux (non masqués) par valeur, du plus fréquent au moins fréquent puis par nom. */
function tally(games: Game[], values: (g: Game) => readonly string[]): { name: string; count: number }[] {
  const counts = new Map<string, number>();
  for (const g of games) {
    if (g.hidden) continue;
    for (const v of new Set(values(g))) counts.set(v, (counts.get(v) ?? 0) + 1);
  }
  return [...counts]
    .map(([name, count]) => ({ name, count }))
    .sort((a, b) => b.count - a.count || titleOrder().compare(a.name, b.name));
}

/** Tous les tags de la bibliothèque et leur nombre de jeux. */
export const libraryTags = (games: Game[]) => tally(games, (g) => g.details?.tags ?? []);

/** Tous les genres de la bibliothèque et leur nombre de jeux. */
export const libraryGenres = (games: Game[]) => tally(games, (g) => g.details?.genres ?? []);

/** Nombre de jeux de chaque ambiance, dans l'ordre fixe des ambiances. */
export function libraryMoods(games: Game[]): { mood: Mood; count: number }[] {
  const counts = new Map(tally(games, (g) => g.details?.moods ?? []).map((t) => [t.name, t.count]));
  return MOODS.map((mood) => ({ mood, count: counts.get(mood) ?? 0 }));
}

/** Préférence d'affichage gardée entre deux lancements (stockage local, facultatif). */
export function remembered<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  try {
    const v = localStorage.getItem(`ludex.${key}`) as T | null;
    return v && allowed.includes(v) ? v : fallback;
  } catch {
    return fallback;
  }
}

/** Valeur gardée telle quelle (section d'une collection, vérifiée plus tard), `null` si absente. */
export function rememberedText(key: string): string | null {
  try {
    return localStorage.getItem(`ludex.${key}`);
  } catch {
    return null;
  }
}

export function remember(key: string, value: string) {
  try {
    localStorage.setItem(`ludex.${key}`, value);
  } catch {
    // Stockage indisponible : la préférence ne survivra pas au redémarrage, sans gravité.
  }
}

/** Tag, ambiance et genre choisis, gardés entre deux lancements. Un tag ou un genre disparu de la
 * bibliothèque reste affiché en pastille (aucun jeu) jusqu'à ce qu'on le retire. */
export function rememberedFacets(): Facets {
  const mood = remembered<Mood | "">("mood", MOODS, "");
  return { tag: rememberedText("tag") || null, mood: mood || null, genre: rememberedText("genre") || null };
}

export function rememberFacets({ tag, mood, genre }: Facets) {
  remember("tag", tag ?? "");
  remember("mood", mood ?? "");
  remember("genre", genre ?? "");
}
