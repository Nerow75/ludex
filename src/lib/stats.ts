// Section « Statistiques » : calculs purs sur la bibliothèque (temps de jeu de chaque jeu) et sur
// les mois renvoyés par Rust (parties mesurées par Ludex, succès Steam).
//
// Deux sources, jamais mélangées :
// - `Game.playtimeMinutes` : un seul chiffre par jeu, celui du launcher (Steam, GOG Galaxy) quand il
//   existe, sinon le cumul mesuré par Ludex (`playtimeSource`). Sert au top, aux launchers et aux genres ;
// - `StatsInfo.months` : sessions de `sessions.json`, découpées au fuseau local. Sert aux mois et aux séries.
import { PLATFORM_LABEL, type Game, type Platform, type PlaytimeSource, type StatsInfo } from "./api";
import { genreLabel } from "./format";
import { locale, t } from "./i18n";

/** Taille du classement des jeux. */
export const TOP_GAMES = 10;

/** Jeux comptés : ni masqués ni outils (Wallpaper Engine fausserait tout). */
export const countedGames = (games: readonly Game[]): Game[] => games.filter((g) => !g.hidden && !g.isTool);

const minutes = (g: Game) => (g.playtimeMinutes !== null && g.playtimeMinutes >= 1 ? g.playtimeMinutes : 0);
/** Ordre des titres de la langue actuelle (un `Intl.Collator` par locale). */
const collators = new Map<string, Intl.Collator>();
function byTitle(): Intl.Collator {
  let c = collators.get(locale());
  if (!c) {
    c = new Intl.Collator(locale(), { sensitivity: "base", numeric: true });
    collators.set(locale(), c);
  }
  return c;
}

export interface PlaytimeTotals {
  /** Somme des temps de chaque jeu (une seule source par jeu). */
  minutes: number;
  /** Jeux joués, dont le temps vient du launcher et dont il vient de Ludex. */
  played: number;
  fromLauncher: number;
  fromLudex: number;
  /** Jeux sans temps connu (launcher qui ne le donne pas, jamais lancé depuis Ludex). */
  unmeasured: number;
}

export function playtimeTotals(games: readonly Game[]): PlaytimeTotals {
  const t: PlaytimeTotals = { minutes: 0, played: 0, fromLauncher: 0, fromLudex: 0, unmeasured: 0 };
  for (const g of countedGames(games)) {
    if (g.playtimeMinutes === null) {
      t.unmeasured += 1;
      continue;
    }
    const m = minutes(g);
    if (m === 0) continue;
    t.minutes += m;
    t.played += 1;
    if (g.playtimeSource === "ludex") t.fromLudex += 1;
    else t.fromLauncher += 1;
  }
  return t;
}

/** Jeux les plus joués selon leur propre compteur, du plus long au plus court (titre à égalité). */
export function topGames(games: readonly Game[], count = TOP_GAMES): Game[] {
  return countedGames(games)
    .filter((g) => minutes(g) > 0)
    .sort((a, b) => minutes(b) - minutes(a) || byTitle().compare(a.title, b.title))
    .slice(0, count);
}

/** « Steam », « GOG Galaxy », « Mesuré par Ludex » : d'où vient le temps d'un jeu. */
export function sourceLabel(game: Game): string {
  const source: PlaytimeSource | null = game.playtimeSource;
  if (source === "ludex") return t.activity.stats.trackedByLudex;
  if (source === "launcher") return game.platform === "gog" ? "GOG Galaxy" : PLATFORM_LABEL[game.platform];
  return t.activity.stats.notTracked;
}

export interface Share<K> {
  key: K;
  label: string;
  minutes: number;
  games: number;
  /** Part du temps total, de 0 à 1 (launchers) ; part du plus grand pour les genres. */
  share: number;
}

/** Temps par launcher : chaque jeu compte dans un seul launcher, les parts font donc 100 %. */
export function launcherShares(games: readonly Game[]): Share<Platform>[] {
  const map = new Map<Platform, Share<Platform>>();
  for (const g of countedGames(games)) {
    const m = minutes(g);
    if (m === 0) continue;
    const s = map.get(g.platform) ?? { key: g.platform, label: PLATFORM_LABEL[g.platform], minutes: 0, games: 0, share: 0 };
    s.minutes += m;
    s.games += 1;
    map.set(g.platform, s);
  }
  const total = [...map.values()].reduce((n, s) => n + s.minutes, 0);
  return [...map.values()]
    .map((s) => ({ ...s, share: total > 0 ? s.minutes / total : 0 }))
    .sort((a, b) => b.minutes - a.minutes || byTitle().compare(a.label, b.label));
}

export interface GenreStats {
  genres: Share<string>[];
  /** Jeux comptés sans genre (pas de fiche Steam ni de genres corrigés). */
  withoutGenre: number;
}

/**
 * Temps et nombre de jeux par genre (genres de la fiche, corrections de l'utilisateur comprises).
 * Un jeu à plusieurs genres compte pour chacun : les parts se lisent par rapport au genre le plus
 * joué, elles ne s'additionnent pas. Genres sans temps gardés (après les autres, par nombre de jeux).
 */
export function genreStats(games: readonly Game[]): GenreStats {
  const map = new Map<string, Share<string>>();
  let withoutGenre = 0;
  for (const g of countedGames(games)) {
    const genres = g.details?.genres ?? [];
    if (genres.length === 0) {
      withoutGenre += 1;
      continue;
    }
    for (const name of new Set(genres)) {
      const s = map.get(name) ?? { key: name, label: genreLabel(name), minutes: 0, games: 0, share: 0 };
      s.minutes += minutes(g);
      s.games += 1;
      map.set(name, s);
    }
  }
  const max = Math.max(0, ...[...map.values()].map((s) => s.minutes));
  const genres = [...map.values()]
    .map((s) => ({ ...s, share: max > 0 ? s.minutes / max : 0 }))
    .sort((a, b) => b.minutes - a.minutes || b.games - a.games || byTitle().compare(a.label, b.label));
  return { genres, withoutGenre };
}

export interface MonthBar {
  /** « nov. » sans le point, « janv. 27 » pour un janvier qui n'est pas le premier mois montré. */
  label: string;
  /** « Novembre 2026 ». */
  title: string;
  value: number;
  current: boolean;
}

/** Formats des mois de la langue actuelle, gardés par locale. */
const monthFormats = new Map<string, { short: Intl.DateTimeFormat; long: Intl.DateTimeFormat }>();
function monthFormat() {
  let f = monthFormats.get(locale());
  if (!f) {
    f = { short: new Intl.DateTimeFormat(locale(), { month: "short" }), long: new Intl.DateTimeFormat(locale(), { month: "long", year: "numeric" }) };
    monthFormats.set(locale(), f);
  }
  return f;
}
const capitalize = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

/** Date du 1er d'un mois `AAAA-MM` (heure locale). */
export function parseMonth(month: string): Date {
  const [y, m] = month.split("-").map(Number);
  return new Date(y, (m || 1) - 1, 1);
}

/** Barres d'un histogramme mensuel ; le dernier mois est le mois en cours. */
export function monthBars(months: StatsInfo["months"], pick: (m: StatsInfo["months"][number]) => number): MonthBar[] {
  const f = monthFormat();
  return months.map((m, i) => {
    const d = parseMonth(m.month);
    return {
      label: f.short.format(d).replace(".", ""),
      title: capitalize(f.long.format(d)),
      value: pick(m),
      current: i === months.length - 1,
    };
  });
}

/** Mois le plus haut (le plus récent à égalité), `null` si tout est à zéro. */
export function peak(bars: readonly MonthBar[]): MonthBar | null {
  let best: MonthBar | null = null;
  for (const b of bars) if (b.value > 0 && (!best || b.value >= best.value)) best = b;
  return best;
}

/** Rien à montrer du tout : ni temps de jeu connu, ni partie mesurée par Ludex, ni succès dans l'année. */
export function statsEmpty(games: readonly Game[], info: StatsInfo): boolean {
  return playtimeTotals(games).played === 0 && info.historySessions === 0 && info.months.every((m) => m.achievements === 0);
}
