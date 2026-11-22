import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Game } from "./api";
import { byTitle, recentGames, selectGames } from "./library";

/** Jeux du dernier scan, sans les jeux masqués (aucun scan lancé). */
export const quickGames = () => invoke<Game[]>("quick_games");
/** Cache la fenêtre de la recherche rapide. */
export const hideQuick = () => invoke<void>("hide_quick");
/** La fenêtre vient d'apparaître (raccourci global). */
export const onQuickShow = (handler: () => void): Promise<UnlistenFn> => listen("quick-show", () => handler());
/** Bibliothèque relue (jeu installé, retour sur Ludex) : la liste est redemandée. */
export const onLibraryChanged = (handler: () => void): Promise<UnlistenFn> => listen("library-changed", () => handler());

/** Résultats montrés au plus. */
export const QUICK_LIMIT = 30;

/**
 * Résultats de la recherche rapide. Sans recherche : les derniers jeux joués, puis les autres par
 * titre. Avec : même recherche que la bibliothèque (titre d'abord, puis studio, éditeur, tag).
 */
export function quickResults(games: Game[], query: string, limit = QUICK_LIMIT): Game[] {
  if (query.trim()) return selectGames(games, "all", "recent", query).slice(0, limit);
  const recent = recentGames(games, limit);
  if (recent.length >= limit) return recent;
  const seen = new Set(recent.map((g) => g.id));
  const rest = games.filter((g) => !g.hidden && !seen.has(g.id)).sort(byTitle);
  return [...recent, ...rest].slice(0, limit);
}
