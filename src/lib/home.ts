import { MOODS, type Game, type Mood } from "./api";
import { cleanTitle } from "./format";

/** Calculs de « Pour toi », à partir des jeux déjà chargés. */

const DAY = 86400;
/** « En cours » joué depuis moins longtemps que ça : il passe avant le dernier jeu lancé. */
const PLAYING_RECENT_DAYS = 30;
/** Joué plus récemment que ça : il passe en dernier dans les idées par ambiance. */
const FRESH_DAYS = 30;

const now = () => Math.floor(Date.now() / 1000);
const byTitle = (a: Game, b: Game) => cleanTitle(a.title).localeCompare(cleanTitle(b.title), "fr", { sensitivity: "base" });
const byRecent = (a: Game, b: Game) => (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0) || byTitle(a, b);

/** Jeux proposés sur l'accueil : ni masqués ni outils. */
const visible = (games: Game[]) => games.filter((g) => !g.hidden && !g.isTool);

/** Jamais lancé, ni par le launcher ni par Ludex. */
export const neverPlayed = (g: Game) => !g.lastPlayed && !(g.playtimeMinutes && g.playtimeMinutes > 0);

export type TonightReason = "live" | "playing" | "last" | "new";
export interface Tonight {
  game: Game;
  reason: TonightReason;
}

/**
 * Jeu de la bannière « Ce soir » : une partie en cours, sinon un jeu « En cours » joué ces
 * 30 derniers jours, sinon le dernier jeu lancé, sinon le premier de la pile.
 */
export function pickTonight(games: Game[], playing: ReadonlySet<string>, pile: Game[]): Tonight | null {
  const list = visible(games);
  const live = list.filter((g) => playing.has(g.id)).sort(byRecent)[0];
  if (live) return { game: live, reason: "live" };
  const since = now() - PLAYING_RECENT_DAYS * DAY;
  const current = list.filter((g) => g.status === "playing" && (g.lastPlayed ?? 0) >= since).sort(byRecent)[0];
  if (current) return { game: current, reason: "playing" };
  const last = list.filter((g) => g.lastPlayed).sort(byRecent)[0];
  if (last) return { game: last, reason: "last" };
  return pile[0] ? { game: pile[0], reason: "new" } : null;
}

/**
 * Bannière « Ce soir » montrable pendant le premier affichage du démarrage (`partial` : EA,
 * Battle.net et « Autres » encore en lecture) ? Seule une partie en cours ne peut pas être
 * détrônée par ces jeux ; sinon la bannière attend le scan complet (place gardée, sans saut).
 */
export function tonightSettled(tonight: Tonight | null, partial: boolean): boolean {
  return !partial || tonight?.reason === "live";
}

/**
 * « Ta pile » : jeux jamais lancés et jeux « À faire » (ni terminés, ni abandonnés, ni en cours),
 * les plus récemment installés d'abord quand la date est connue, puis par titre.
 */
export function pileGames(games: Game[], installed: ReadonlyMap<string, number>): Game[] {
  return visible(games)
    .filter((g) => g.status === "todo" || (!g.status && neverPlayed(g)))
    .sort((a, b) => (installed.get(b.id) ?? 0) - (installed.get(a.id) ?? 0) || byTitle(a, b));
}

export interface MoodIdeas {
  mood: Mood;
  /** Jeux de l'ambiance dans la bibliothèque (non masqués). */
  count: number;
  games: Game[];
}

/**
 * Quelques jeux par ambiance. D'abord ceux qui n'ont pas été lancés depuis longtemps (le plus
 * ancien d'abord), puis ceux jamais lancés, puis ceux joués ce mois-ci. Un jeu déjà proposé dans
 * une ambiance précédente (ou exclu : la bannière) passe après les autres. Les abandonnés sont écartés.
 */
export function moodIdeas(games: Game[], perMood: number, exclude: ReadonlySet<string>): MoodIdeas[] {
  const fresh = now() - FRESH_DAYS * DAY;
  const rank = (g: Game) => (!g.lastPlayed ? 1 : g.lastPlayed < fresh ? 0 : 2);
  const shown = new Set(exclude);
  const list = visible(games);
  return MOODS.map((mood) => {
    const all = list.filter((g) => g.details?.moods.includes(mood));
    const picked = all
      .filter((g) => g.status !== "dropped" && !exclude.has(g.id))
      .sort((a, b) =>
        Number(shown.has(a.id)) - Number(shown.has(b.id)) ||
        rank(a) - rank(b) ||
        (a.lastPlayed ?? 0) - (b.lastPlayed ?? 0) ||
        byTitle(a, b),
      )
      .slice(0, perMood);
    for (const g of picked) shown.add(g.id);
    return { mood, count: all.length, games: picked };
  }).filter((m) => m.games.length > 0);
}

/** Téléchargements et mises à jour en cours chez le launcher (pas « Mise à jour requise » : elle se fait au lancement). */
export const downloads = (games: Game[]) =>
  visible(games)
    .filter((g) => ["updating", "paused", "queued"].includes(g.installState.kind))
    .sort(byTitle);

/** Dernier succès Steam débloqué dans la bibliothèque (jeux non masqués). */
export function lastAchievement(games: Game[]): { game: Game; at: number } | null {
  let best: { game: Game; at: number } | null = null;
  for (const g of games) {
    const at = g.achievements?.lastUnlocked;
    if (!g.hidden && at && (!best || at > best.at)) best = { game: g, at };
  }
  return best;
}
