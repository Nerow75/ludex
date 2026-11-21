// Textes pour les lecteurs d'écran (Narrateur, NVDA) : noms accessibles des jaquettes et des lignes,
// annonces des changements de la bibliothèque. Fonctions pures, testées par Vitest.
import { PLATFORM_LABEL, type Game } from "./api";
import { formatAchievements, formatPlaytime, installBadge, relativeTime } from "./format";
import { achievementPercent } from "./library";

/** « 1 jeu », « 12 jeux ». */
export const gamesCount = (n: number) => `${n} jeu${n > 1 ? "x" : ""}`;

/** État court du jeu : « en jeu », sinon la mention d'installation en minuscules, sinon rien. */
function stateOf(game: Game, playing: boolean): string | null {
  if (playing) return "en jeu";
  const badge = installBadge(game.installState);
  return badge ? badge.toLowerCase() : null;
}

/**
 * Nom accessible d'une jaquette de la grille : ce que l'œil lit sur la carte (titre, launcher, étoile,
 * mention, pourcentage des succès, ligne sous le titre), dans cet ordre.
 * « Hades, Steam, favori, il y a 2 jours, succès 80 % ».
 */
export function cardLabel(game: Game, playing: boolean, meta: string): string {
  const parts = [game.title, PLATFORM_LABEL[game.platform]];
  if (game.favorite) parts.push("favori");
  const state = stateOf(game, playing);
  if (state) parts.push(state);
  else if (meta.trim()) parts.push(meta.trim());
  const percent = achievementPercent(game);
  if (percent !== null) parts.push(`succès ${percent} %`);
  return parts.join(", ");
}

/**
 * Nom accessible d'une ligne de la vue liste : le bouton couvre toute la ligne, son nom doit donc
 * reprendre les colonnes utiles (le nom remplace le contenu pour le lecteur d'écran).
 * « Hades, favori, Steam, temps de jeu 12 h, joué il y a 2 jours, succès 40 / 49 ».
 */
export function rowLabel(game: Game, playing: boolean): string {
  const parts = [game.title];
  if (game.favorite) parts.push("favori");
  const state = stateOf(game, playing);
  if (state) parts.push(state);
  parts.push(PLATFORM_LABEL[game.platform]);
  if (game.playtimeMinutes !== null && game.playtimeMinutes >= 1) parts.push(`temps de jeu ${formatPlaytime(game.playtimeMinutes)}`);
  parts.push(game.lastPlayed ? `joué ${relativeTime(game.lastPlayed)}` : "jamais lancé");
  const ach = game.achievements;
  if (ach && ach.total > 0) parts.push(`succès ${formatAchievements(ach.unlocked, ach.total)}`);
  return parts.join(", ");
}

/**
 * Annonce de la bibliothèque affichée, lue par une région `aria-live` discrète quand la recherche,
 * le filtre, la section ou le nombre de jeux change : « 12 jeux pour « hades » », « Tous les jeux,
 * filtre Steam : 69 jeux », « Aucun jeu ne correspond à « zzz » ».
 */
export function libraryAnnouncement(o: { section: string; count: number; query: string; filter: string | null; extra?: readonly string[] }): string {
  const q = o.query.trim();
  if (q) return o.count === 0 ? `Aucun jeu ne correspond à « ${q} »` : `${gamesCount(o.count)} pour « ${q} »`;
  const filters = [o.filter ? `filtre ${o.filter}` : null, ...(o.extra ?? [])].filter(Boolean);
  const head = filters.length ? `${o.section}, ${filters.join(", ")}` : o.section;
  return `${head} : ${o.count === 0 ? "aucun jeu" : gamesCount(o.count)}`;
}

/** Position dans une suite (visionneuse) : « Capture 3 sur 128 ». */
export const positionText = (what: string, index: number, count: number) => `${what} ${index + 1} sur ${count}`;

/**
 * Élément suivant pour Tab ou Maj+Tab dans un piège à focus (dialogue modal) : boucle d'un bout à
 * l'autre. `current` = index de l'élément qui a le focus (-1 : hors de la liste). `null` : liste vide.
 */
export function trapNext(count: number, current: number, backwards: boolean): number | null {
  if (count === 0) return null;
  if (current < 0) return backwards ? count - 1 : 0;
  return backwards ? (current - 1 + count) % count : (current + 1) % count;
}
