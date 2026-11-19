import { MOODS, STATUS_LABEL, STATUSES, type Game, type Mood, type Platform, type Status } from "./api";
import { cleanTitle } from "./format";
import { prepareQuery, searchRank } from "./search";

export type Filter = "all" | "favorites" | "hidden" | Platform;
export type Sort = "recent" | "playtime" | "title" | "size";

/** Filtres en plus du launcher : un tag Steam, une ambiance. `null` : pas de filtre. */
export interface Facets {
  tag: string | null;
  mood: Mood | null;
}
export const NO_FACETS: Facets = { tag: null, mood: null };

export const SORT_LABEL: Record<Sort, string> = { recent: "Récents", playtime: "Plus joués", title: "A → Z", size: "Taille" };

const byTitle = (a: Game, b: Game) => cleanTitle(a.title).localeCompare(cleanTitle(b.title), "fr", { sensitivity: "base" });

const COMPARE: Record<Sort, (a: Game, b: Game) => number> = {
  recent: (a, b) => (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0) || byTitle(a, b),
  // Jamais joué (0) avant inconnu (null).
  playtime: (a, b) => (b.playtimeMinutes ?? -1) - (a.playtimeMinutes ?? -1) || byTitle(a, b),
  title: byTitle,
  size: (a, b) => (b.sizeOnDisk ?? 0) - (a.sizeOnDisk ?? 0) || byTitle(a, b),
};

/** Filtre par statut de progression, en plus du filtre par launcher. */
export type StatusFilter = "any" | "none" | Status;
export const STATUS_FILTERS: readonly StatusFilter[] = ["any", ...STATUSES, "none"];
export const STATUS_FILTER_LABEL: Record<StatusFilter, string> = { any: "Tous les statuts", none: "Sans statut", ...STATUS_LABEL };

export const matchesStatus = (game: Game, filter: StatusFilter) =>
  filter === "any" || (filter === "none" ? !game.status : game.status === filter);

export function matchesFilter(game: Game, filter: Filter): boolean {
  if (filter === "hidden") return game.hidden;
  if (game.hidden) return false;
  if (filter === "favorites") return game.favorite;
  return filter === "all" || game.platform === filter;
}

export function matchesFacets(game: Game, { tag, mood }: Facets): boolean {
  return (!tag || !!game.details?.tags.includes(tag)) && (!mood || !!game.details?.moods.includes(mood));
}

export const hasFacets = (f: Facets) => f.tag !== null || f.mood !== null;

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
    .sort((a, b) => b.count - a.count || a.name.localeCompare(b.name, "fr", { sensitivity: "base" }));
}

/** Tous les tags de la bibliothèque et leur nombre de jeux. */
export const libraryTags = (games: Game[]) => tally(games, (g) => g.details?.tags ?? []);

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

/** Tag et ambiance choisis, gardés entre deux lancements. Un tag disparu de la bibliothèque reste
 * affiché en pastille (aucun jeu) jusqu'à ce qu'on le retire. */
export function rememberedFacets(): Facets {
  let tag: string | null = null;
  try {
    tag = localStorage.getItem("ludex.tag") || null;
  } catch {
    // Stockage indisponible : pas de tag retenu.
  }
  const mood = remembered<Mood | "">("mood", MOODS, "");
  return { tag, mood: mood || null };
}

export function rememberFacets({ tag, mood }: Facets) {
  remember("tag", tag ?? "");
  remember("mood", mood ?? "");
}
