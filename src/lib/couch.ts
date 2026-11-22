import { MOOD_LABEL, type Game } from "./api";
import type { PadAction } from "./gamepad";
import { moodIdeas, pickTonight, pileGames } from "./home";
import { t } from "./i18n";
import { recentGames, selectGames } from "./library";

/** Calculs du mode canapé, à partir des jeux déjà chargés et des calculs de « Pour toi ». */

/** Jeux par rangée (sauf « Tous les jeux », complète). */
export const COUCH_PER_ROW = 20;

export interface CouchRow {
  /** `resume`, `recent`, `favorites`, `mood:<ambiance>`, `all`. */
  id: string;
  title: string;
  games: Game[];
}

/**
 * Rangées du mode canapé, dans l'ordre : Reprendre (parties en cours et jeux « En cours »),
 * Favoris, Récents (sans ceux de Reprendre), une rangée par ambiance, Tous les jeux (A → Z).
 * Rangée vide = absente. Jeux masqués jamais proposés, outils seulement dans « Tous les jeux »
 * s'ils ont été ré-affichés (comme la bibliothèque).
 */
export function couchRows(games: Game[], playing: ReadonlySet<string>, perRow = COUCH_PER_ROW): CouchRow[] {
  const visible = games.filter((g) => !g.hidden && !g.isTool);
  const byRecent = (a: Game, b: Game) => (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0);
  const resume = visible
    .filter((g) => playing.has(g.id) || g.status === "playing")
    .sort((a, b) => Number(playing.has(b.id)) - Number(playing.has(a.id)) || byRecent(a, b))
    .slice(0, perRow);
  const inResume = new Set(resume.map((g) => g.id));
  const recent = recentGames(visible, perRow + resume.length).filter((g) => !inResume.has(g.id)).slice(0, perRow);
  const favorites = selectGames(visible, "favorites", "recent", "").slice(0, perRow);
  const moods = moodIdeas(games, perRow, new Set()).map((m) => ({ id: `mood:${m.mood}`, title: MOOD_LABEL[m.mood], games: m.games }));
  const rows: CouchRow[] = [
    { id: "resume", title: t.extras.couch.rows.resume, games: resume },
    { id: "favorites", title: t.extras.couch.rows.favorites, games: favorites },
    { id: "recent", title: t.extras.couch.rows.recent, games: recent },
    ...moods,
    { id: "all", title: t.extras.couch.rows.all, games: selectGames(games, "all", "title", "") },
  ];
  return rows.filter((r) => r.games.length > 0);
}

/** Clé d'une jaquette : un même jeu peut être dans plusieurs rangées. */
export const couchKey = (rowId: string, gameId: string) => `${rowId}/${gameId}`;

/**
 * Jaquette qui a le focus à l'ouverture : le jeu de la bannière « Ce soir » de « Pour toi »
 * (partie en cours, jeu « En cours », dernier lancé, premier de la pile) dans la première rangée
 * qui le contient, sinon la première jaquette. `null` : aucune rangée.
 */
export function initialFocus(rows: CouchRow[], games: Game[], playing: ReadonlySet<string>): string | null {
  const tonight = pickTonight(games, playing, pileGames(games, new Map()));
  const row = tonight && rows.find((r) => r.games.some((g) => g.id === tonight.game.id));
  if (row && tonight) return couchKey(row.id, tonight.game.id);
  const first = rows[0];
  return first ? couchKey(first.id, first.games[0].id) : null;
}

/** Ce que le mode canapé sait faire, au clavier comme à la manette. */
export type CouchAction =
  | "up"
  | "down"
  | "left"
  | "right"
  | "confirm"
  | "fiche"
  | "favorite"
  | "prevRow"
  | "nextRow"
  | "first"
  | "last"
  | "back"
  | "fullscreen"
  | "exit";

interface KeyLike {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
}

/**
 * Touche du clavier : flèches, Entrée (jouer, ou le bouton choisi dans la fiche), Espace (fiche),
 * Ctrl+D (favori, comme dans la bibliothèque), Page précédente / suivante (rangée), Début / Fin
 * (bout de la rangée), Échap ou Retour arrière (retour, puis sortie), F11 (vrai plein écran si le
 * réglage le permet, sinon sortie). `null` : rien.
 */
export function couchKeyAction(e: KeyLike): CouchAction | null {
  const ctrl = !!(e.ctrlKey || e.metaKey);
  if (e.altKey) return null;
  if (ctrl) return e.key.toLowerCase() === "d" ? "favorite" : null;
  switch (e.key) {
    case "ArrowUp": return "up";
    case "ArrowDown": return "down";
    case "ArrowLeft": return "left";
    case "ArrowRight": return "right";
    case "Enter": return "confirm";
    case " ": return "fiche";
    case "PageUp": return "prevRow";
    case "PageDown": return "nextRow";
    case "Home": return "first";
    case "End": return "last";
    case "Escape":
    case "Backspace": return "back";
    case "F11": return "fullscreen";
    default: return null;
  }
}

/**
 * Bouton de la manette : A jouer, X fiche, Y favori, B retour, LB / RB rangée, Guide ou Vue + Menu
 * sortie. Gâchettes, Vue et Menu seuls : rien (aucun filtre ni tri dans le mode canapé).
 */
export function couchPadAction(a: PadAction): CouchAction | null {
  switch (a) {
    case "up":
    case "down":
    case "left":
    case "right":
      return a;
    case "confirm": return "confirm";
    case "details": return "fiche";
    case "favorite": return "favorite";
    case "back": return "back";
    case "prevSection": return "prevRow";
    case "nextSection": return "nextRow";
    case "couch": return "exit";
    default: return null;
  }
}

/** Date locale `AAAA-MM-JJ`. */
export function localDate(d: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/**
 * Temps de jeu d'aujourd'hui, en secondes, lu dans le récap de la semaine en cours (`start` = lundi,
 * `days` = temps de chaque jour). 0 si aujourd'hui n'est pas dans la période.
 */
export function todaySecs(recap: { start: string; days: number[] }, today: Date): number {
  const [y, m, d] = recap.start.split("-").map(Number);
  const day = Math.round((Date.UTC(today.getFullYear(), today.getMonth(), today.getDate()) - Date.UTC(y, m - 1, d)) / 86_400_000);
  return day >= 0 ? recap.days[day] ?? 0 : 0;
}

/** Rangée voisine pour LB / RB, sans boucler. `null` : déjà au bout. */
export function stepRow(count: number, current: number, delta: number): number | null {
  const next = current + delta;
  return next >= 0 && next < count ? next : null;
}

/** Préférence retenue : Ludex rouvert en mode canapé s'il a été quitté ainsi. */
export const COUCH_KEY = "couch";
export const COUCH_VALUES = ["on", "off"] as const;

/** Particule du fond « Particules » du mode canapé (`CouchFx`). */
export interface Dust {
  /** Position de départ, en % de la largeur. */
  left: number;
  /** Diamètre, en pixels. */
  size: number;
  /** Durée d'une montée, en secondes. */
  duration: number;
  /** Avance dans le cycle (secondes) : l'écran est déjà peuplé à l'ouverture. */
  delay: number;
  /** Décalage horizontal pendant la montée, en vw. */
  drift: number;
  opacity: number;
}

/**
 * Particules réparties sans tirage au hasard (suite de la proportion dorée) : même fond à chaque
 * ouverture, aucune boucle ni calcul pendant l'animation (tout en CSS).
 */
export function dustParticles(count: number): Dust[] {
  const golden = 0.618033988749895;
  return Array.from({ length: Math.max(0, Math.floor(count)) }, (_, i) => {
    const a = (i * golden) % 1;
    const b = (i * golden * golden + 0.37) % 1;
    const duration = Math.round(18 + b * 16);
    return {
      left: Math.round(a * 1000) / 10,
      size: 2 + Math.round(b * 3),
      duration,
      delay: Math.round(((i * 0.414) % 1) * duration * 10) / 10,
      drift: Math.round((a - 0.5) * 120) / 10,
      opacity: Math.round((0.18 + b * 0.22) * 100) / 100,
    };
  });
}
