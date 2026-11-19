import { STATUS_LABEL, STATUSES, type Game, type Platform, type Status } from "./api";
import { cleanTitle, norm } from "./format";

export type Filter = "all" | "favorites" | "hidden" | Platform;
export type Sort = "recent" | "playtime" | "title" | "size";

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

export function selectGames(games: Game[], filter: Filter, sort: Sort, query: string): Game[] {
  const q = norm(query.trim());
  return games
    .filter((g) => matchesFilter(g, filter) && (!q || norm(cleanTitle(g.title)).includes(q)))
    .sort(COMPARE[sort]);
}

/** Les derniers jeux joués, hors jeux masqués. */
export const recentGames = (games: Game[], n: number) =>
  games.filter((g) => !g.hidden && g.lastPlayed).sort(COMPARE.recent).slice(0, n);

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
