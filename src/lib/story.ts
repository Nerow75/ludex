// Story du récap : pages verticales tirées du récap d'une période (fonctions pures, testées par
// Vitest). Chaque page n'existe que si sa donnée existe ; l'écran (`StoryView`) et l'image exportée
// (`storyImage.ts`) dessinent les mêmes pages.
import type { RareUnlock, Recap, RecapPeriod } from "./api";
import { formatDuration } from "./format";

/** Durée d'une page quand la story avance seule. */
export const STORY_PAGE_MS = 5000;
/** Nombre de pages au plus. */
export const MAX_PAGES = 9;

export type StoryKind = "total" | "top" | "rhythm" | "longest" | "streak" | "discoveries" | "achievements" | "profile" | "end";

/** Ligne d'une liste (top des jeux, nouveaux jeux). `share` : longueur de la barre, de 0 à 1. */
export interface StoryItem {
  gameId: string;
  label: string;
  value: string;
  share: number;
}

/** Barre du rythme (jours de la semaine). */
export interface StoryBar {
  label: string;
  share: number;
  active: boolean;
}

export interface StoryPage {
  kind: StoryKind;
  /** Jeu dont l'image sert de fond et dont le logo est montré (`null` : fond uni). */
  gameId: string | null;
  /** Petite ligne du haut. */
  eyebrow: string;
  /** Grand chiffre ou grand mot, s'il y en a un. */
  value: string | null;
  title: string;
  detail: string | null;
  items: StoryItem[];
  bars: StoryBar[];
  achievement: RareUnlock | null;
  /** Logo du jeu de fond montré au-dessus du texte. */
  logo: boolean;
}

export type ProfileId = "night" | "early" | "marathon" | "explorer" | "hunter" | "regular" | "loyal" | "weekend" | "zapper" | "casual";

export interface Profile {
  id: ProfileId;
  title: string;
  line: string;
}

interface Words {
  /** « cette semaine », « cette semaine-là ». */
  current: string;
  past: string;
  /** « la semaine précédente », puis « à la semaine précédente » (« au mois précédent »). */
  previous: string;
  versus: string;
  /** Fin de la story de la période en cours. */
  next: string;
  /** Seuils du profil, qui grandissent avec la période. */
  explorer: number;
  hunter: number;
  regular: number;
  zapper: number;
}

const WORDS: Record<RecapPeriod, Words> = {
  week: { current: "cette semaine", past: "cette semaine-là", previous: "la semaine précédente", versus: "à la semaine précédente", next: "À la semaine prochaine", explorer: 2, hunter: 10, regular: 5, zapper: 4 },
  month: { current: "ce mois-ci", past: "ce mois-là", previous: "le mois précédent", versus: "au mois précédent", next: "Au mois prochain", explorer: 3, hunter: 25, regular: 10, zapper: 6 },
  year: { current: "cette année", past: "cette année-là", previous: "l'année précédente", versus: "à l'année précédente", next: "À l'année prochaine", explorer: 6, hunter: 100, regular: 21, zapper: 12 },
};

const WEEKDAYS = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
const WEEKDAY_LETTERS = ["L", "M", "M", "J", "V", "S", "D"];
const MOMENTS = ["la nuit", "le matin", "l'après-midi", "le soir"];
const MOMENT_HOURS = ["de minuit à 6 h", "de 6 h à midi", "de midi à 18 h", "de 18 h à minuit"];

const integer = new Intl.NumberFormat("fr-FR");
const decimal = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 1 });
const monthYear = new Intl.DateTimeFormat("fr-FR", { month: "long", year: "numeric" });
const dayMonth = new Intl.DateTimeFormat("fr-FR", { day: "numeric", month: "long" });
const dayFull = new Intl.DateTimeFormat("fr-FR", { weekday: "long", day: "numeric", month: "long" });
const hourMinute = new Intl.DateTimeFormat("fr-FR", { hour: "2-digit", minute: "2-digit" });

const capitalize = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
const plural = (n: number, one: string, many: string) => `${integer.format(n)} ${n > 1 ? many : one}`;
const percent = (part: number, whole: number) => (whole > 0 ? Math.round((part / whole) * 100) : 0);
const sum = (values: number[]) => values.reduce((a, b) => a + b, 0);

/** « AAAA-MM-JJ » en date locale (sans passer par UTC). */
export const parseDay = (s: string) => {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, m - 1, d);
};

/** « 16 au 22 novembre 2026 », « Novembre 2026 », « 2026 ». */
export function periodTitle(r: Pick<Recap, "period" | "start" | "end">): string {
  const start = parseDay(r.start);
  const end = parseDay(r.end);
  if (r.period === "year") return String(start.getFullYear());
  if (r.period === "month") return capitalize(monthYear.format(start));
  if (start.getFullYear() !== end.getFullYear()) return `${dayMonth.format(start)} ${start.getFullYear()} au ${dayMonth.format(end)} ${end.getFullYear()}`;
  if (start.getMonth() !== end.getMonth()) return `${dayMonth.format(start)} au ${dayMonth.format(end)} ${end.getFullYear()}`;
  return `${start.getDate()} au ${dayMonth.format(end)} ${end.getFullYear()}`;
}

/** Une story n'a de sens qu'avec du temps de jeu mesuré dans la période. */
export const hasStory = (r: Recap) => r.totalSecs > 0;

/** Comparaison avec la période précédente (au même stade pour la période en cours), `null` sans rien à comparer. */
export function storyComparison(r: Recap): string | null {
  const w = WORDS[r.period];
  const current = r.back === 0 && r.elapsedDays < r.days.length;
  const prev = current ? r.previousToDateSecs : r.previousSecs;
  const when = current ? " au même stade" : "";
  if (prev === 0) return `Rien de mesuré ${w.previous}${when}`;
  const pct = Math.round(((r.totalSecs - prev) / prev) * 100);
  if (pct === 0) return `Autant que ${w.previous}${when}`;
  return `${pct > 0 ? "+" : "−"}${integer.format(Math.abs(pct))} % par rapport ${w.versus}${when}`;
}

/**
 * Profil de joueur tiré du récap : chaque profil a un seuil, le score est la donnée divisée par ce
 * seuil (1 = seuil tout juste atteint) ; le plus haut score l'emporte, l'ordre de la liste départage.
 * Aucun seuil atteint : « Le joueur tranquille ».
 */
export function playerProfile(r: Recap, title: (gameId: string) => string): Profile {
  const w = WORDS[r.period];
  const moments = sum(r.momentSecs);
  const weekdays = sum(r.weekdaySecs);
  const average = r.sessions > 0 ? r.totalSecs / r.sessions : 0;
  const longest = r.longest?.secs ?? 0;
  const top = r.top[0];
  const topShare = top && r.totalSecs > 0 ? top.secs / r.totalSecs : 0;
  const news = r.newGames.length;
  const weekend = r.weekdaySecs[5] + r.weekdaySecs[6];

  const candidates: { profile: Profile; score: number }[] = [
    {
      score: moments > 0 ? r.momentSecs[0] / moments / 0.4 : 0,
      profile: { id: "night", title: "Le noctambule", line: `${percent(r.momentSecs[0], moments)} % de ton temps de jeu entre minuit et 6 h` },
    },
    {
      score: moments > 0 ? r.momentSecs[1] / moments / 0.45 : 0,
      profile: { id: "early", title: "Le lève-tôt", line: `${percent(r.momentSecs[1], moments)} % de ton temps de jeu entre 6 h et midi` },
    },
    {
      score: Math.max(average / 7200, longest / 14400),
      profile: {
        id: "marathon",
        title: "Le marathonien",
        line: `${formatDuration(average || 60)} par session en moyenne, ${formatDuration(longest || 60)} pour la plus longue`,
      },
    },
    {
      score: news >= 0.4 * Math.max(1, r.games) ? news / w.explorer : 0,
      profile: { id: "explorer", title: "L'explorateur", line: `${plural(news, "nouveau jeu lancé", "nouveaux jeux lancés")} ${r.back === 0 ? w.current : w.past}` },
    },
    {
      score: r.achievements / w.hunter,
      profile: { id: "hunter", title: "Le chasseur de succès", line: `${plural(r.achievements, "succès débloqué", "succès débloqués")} ${r.back === 0 ? w.current : w.past}` },
    },
    {
      score: r.bestStreak / w.regular,
      profile: { id: "regular", title: "Le régulier", line: `${plural(r.bestStreak, "jour", "jours")} de jeu d'affilée, sans en sauter un` },
    },
    {
      score: top && r.sessions >= 3 && r.games >= 1 ? topShare / 0.7 : 0,
      profile: { id: "loyal", title: "Le fidèle", line: top ? `${percent(top.secs, r.totalSecs)} % de ton temps sur ${title(top.gameId)}` : "" },
    },
    {
      score: r.sessions >= 3 && weekdays > 0 ? weekend / weekdays / 0.6 : 0,
      profile: { id: "weekend", title: "Le joueur du week-end", line: `${percent(weekend, weekdays)} % de ton temps le samedi et le dimanche` },
    },
    {
      score: topShare < 0.4 ? r.games / w.zapper : 0,
      profile: { id: "zapper", title: "Le touche-à-tout", line: `${plural(r.games, "jeu différent", "jeux différents")}, aucun ne prend le dessus` },
    },
  ];
  let best: { profile: Profile; score: number } | null = null;
  for (const c of candidates) {
    if (c.score >= 1 && (!best || c.score > best.score)) best = c;
  }
  return best?.profile ?? { id: "casual", title: "Le joueur tranquille", line: `${plural(r.sessions, "session", "sessions")}, sans se presser` };
}

export interface StoryContext {
  /** Titre d'un jeu (« Jeu retiré de la bibliothèque » s'il n'y est plus). */
  title: (gameId: string) => string;
  /** Le jeu est encore dans la bibliothèque (sinon pas d'image de fond). */
  known: (gameId: string) => boolean;
  /** Succès le plus rare de la période, si le cache des pourcentages le connaît. */
  rarest: RareUnlock | null;
}

const page = (kind: StoryKind, fields: Partial<StoryPage>): StoryPage => ({
  kind,
  gameId: null,
  eyebrow: "",
  value: null,
  title: "",
  detail: null,
  items: [],
  bars: [],
  achievement: null,
  logo: false,
  ...fields,
});

/** Pages de la story, dans l'ordre ; vide si la période n'a aucun temps de jeu. */
export function storyPages(r: Recap, ctx: StoryContext): StoryPage[] {
  if (!hasStory(r)) return [];
  const w = WORDS[r.period];
  const when = r.back === 0 ? w.current : w.past;
  const game = (id: string | undefined | null) => (id && ctx.known(id) ? id : null);
  const first = r.top[0]?.gameId ?? null;
  const pages: StoryPage[] = [];

  pages.push(
    page("total", {
      gameId: game(first),
      eyebrow: periodTitle(r),
      value: formatDuration(r.totalSecs),
      title: `de jeu ${when}`,
      detail: storyComparison(r),
    }),
  );

  if (r.top.length > 0) {
    const max = Math.max(1, r.top[0].secs);
    pages.push(
      page("top", {
        gameId: game(first),
        eyebrow: "Les plus joués",
        title: r.top.length === 1 ? `Ton jeu ${when}` : `Ton top ${r.top.length}`,
        items: r.top.map((g) => ({ gameId: g.gameId, label: ctx.title(g.gameId), value: formatDuration(g.secs), share: g.secs / max })),
      }),
    );
  }

  if (r.sessions >= 2 && r.favoriteWeekday !== null) {
    const max = Math.max(1, ...r.weekdaySecs);
    const moment = r.favoriteMoment;
    pages.push(
      page("rhythm", {
        gameId: game(r.top[1]?.gameId ?? first),
        eyebrow: "Ton rythme",
        value: capitalize(WEEKDAYS[r.favoriteWeekday]),
        title: "Ton jour de jeu préféré",
        detail: moment === null ? null : `Et surtout ${MOMENTS[moment]}, ${MOMENT_HOURS[moment]}`,
        bars: r.weekdaySecs.map((secs, i) => ({ label: WEEKDAY_LETTERS[i], share: secs / max, active: i === r.favoriteWeekday })),
      }),
    );
  }

  if (r.longest && r.sessions >= 2) {
    const start = new Date(r.longest.start * 1000);
    pages.push(
      page("longest", {
        gameId: game(r.longest.gameId),
        eyebrow: "Ta plus longue session",
        value: formatDuration(r.longest.secs),
        title: ctx.title(r.longest.gameId),
        detail: `${capitalize(dayFull.format(start))}, à partir de ${hourMinute.format(start)}`,
        logo: true,
      }),
    );
  }

  if (r.bestStreak >= 2) {
    const running = r.back === 0 && r.currentStreak >= 2;
    pages.push(
      page("streak", {
        gameId: game(r.top[2]?.gameId ?? first),
        eyebrow: "Ta série",
        value: plural(r.bestStreak, "jour", "jours"),
        title: "de jeu d'affilée",
        detail: running ? `Série en cours : ${plural(r.currentStreak, "jour", "jours")}` : `Ta plus longue série ${when}`,
      }),
    );
  }

  if (r.newGames.length > 0) {
    const n = r.newGames.length;
    pages.push(
      page("discoveries", {
        gameId: game(r.newGames[0].gameId),
        eyebrow: "Nouveautés",
        value: integer.format(n),
        title: n > 1 ? "nouveaux jeux lancés" : "nouveau jeu lancé",
        detail: n > 5 ? `Les 5 premiers, et ${plural(n - 5, "autre", "autres")}` : null,
        items: r.newGames.slice(0, 5).map((g) => ({ gameId: g.gameId, label: ctx.title(g.gameId), value: dayMonth.format(new Date(g.firstPlayed * 1000)), share: 0 })),
      }),
    );
  }

  if (r.achievements > 0) {
    const most = r.achievementGames[0];
    const rare = ctx.rarest;
    pages.push(
      page("achievements", {
        gameId: game(rare?.gameId ?? most?.gameId),
        eyebrow: "Succès Steam",
        value: integer.format(r.achievements),
        title: r.achievements > 1 ? "succès débloqués" : "succès débloqué",
        detail: most && r.achievementGames.length > 1 ? `dont ${integer.format(most.count)} dans ${ctx.title(most.gameId)}` : most ? `dans ${ctx.title(most.gameId)}` : null,
        achievement: rare,
      }),
    );
  }

  const profile = playerProfile(r, ctx.title);
  pages.push(
    page("profile", {
      gameId: game(r.top[3]?.gameId ?? first),
      eyebrow: "Ton profil de joueur",
      title: profile.title,
      detail: profile.line,
    }),
  );

  pages.push(
    page("end", {
      gameId: game(first),
      eyebrow: periodTitle(r),
      value: formatDuration(r.totalSecs),
      title: r.back === 0 ? w.next : "Fin du récap",
      detail: `${plural(r.sessions, "session", "sessions")} · ${plural(r.games, "jeu", "jeux")}`,
    }),
  );

  return pages.slice(0, MAX_PAGES);
}

/** « Débloqué par 1,2 % des joueurs Steam ». */
export const rarityText = (percent: number) => `Débloqué par ${decimal.format(percent < 0.1 ? 0.1 : percent)} % des joueurs Steam${percent < 0.1 ? " ou moins" : ""}`;

export type StoryAction = "next" | "prev" | "pause" | "close";

/** Touche pendant la story (la manette arrive par les mêmes touches, voir `PAD_MENU_KEYS`). */
export function storyAction(key: string): StoryAction | null {
  switch (key) {
    case "ArrowRight":
    case "ArrowDown":
    case "Enter":
    case "PageDown":
      return "next";
    case "ArrowLeft":
    case "ArrowUp":
    case "PageUp":
      return "prev";
    case " ":
    case "Spacebar":
      return "pause";
    case "Escape":
    case "Backspace":
      return "close";
    default:
      return null;
  }
}

/** Page suivante ou précédente, bornée ; `null` après la dernière (la story se ferme). */
export function stepPage(index: number, count: number, action: "next" | "prev"): number | null {
  if (action === "prev") return Math.max(0, index - 1);
  return index + 1 < count ? index + 1 : null;
}
