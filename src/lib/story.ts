// Story du récap : pages verticales tirées du récap d'une période (fonctions pures, testées par
// Vitest). Chaque page n'existe que si sa donnée existe ; l'écran (`StoryView`) et l'image exportée
// (`storyImage.ts`) dessinent les mêmes pages.
import { PLATFORM_LABEL, type AchievementStore, type RareUnlock, type Recap, type RecapPeriod } from "./api";
import { formatDuration } from "./format";
import { formatNumber, lang, locale, t } from "./i18n";

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

/** Seuils du profil, qui grandissent avec la période. */
interface Thresholds {
  explorer: number;
  hunter: number;
  regular: number;
  zapper: number;
}

const THRESHOLDS: Record<RecapPeriod, Thresholds> = {
  week: { explorer: 2, hunter: 10, regular: 5, zapper: 4 },
  month: { explorer: 3, hunter: 25, regular: 10, zapper: 6 },
  year: { explorer: 6, hunter: 100, regular: 21, zapper: 12 },
};

/** Mots de la période (« cette semaine », « à la semaine précédente »…), dans la langue actuelle. */
const words = (period: RecapPeriod) => t.activity.period[period];

/** Formats de dates de la langue actuelle, gardés par locale et options. */
const dateFormats = new Map<string, Intl.DateTimeFormat>();
function df(options: Intl.DateTimeFormatOptions): Intl.DateTimeFormat {
  const key = `${locale()}|${JSON.stringify(options)}`;
  let f = dateFormats.get(key);
  if (!f) {
    f = new Intl.DateTimeFormat(locale(), options);
    dateFormats.set(key, f);
  }
  return f;
}
const monthYear = () => df({ month: "long", year: "numeric" });
const dayMonth = () => df({ day: "numeric", month: "long" });
const dayFull = () => df({ weekday: "long", day: "numeric", month: "long" });
/** « 20:30 » en français, « 8:30 PM » en anglais. */
const hourMinute = () => df(lang() === "en" ? { hour: "numeric", minute: "2-digit" } : { hour: "2-digit", minute: "2-digit" });

const capitalize = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
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
  if (r.period === "month") return capitalize(monthYear().format(start));
  return t.activity.weekRange(start, end);
}

/** Une story n'a de sens qu'avec du temps de jeu mesuré dans la période. */
export const hasStory = (r: Recap) => r.totalSecs > 0;

/** Comparaison avec la période précédente (au même stade pour la période en cours), `null` sans rien à comparer. */
export function storyComparison(r: Recap): string | null {
  const w = words(r.period);
  const c = t.activity.compare;
  const current = r.back === 0 && r.elapsedDays < r.days.length;
  const prev = current ? r.previousToDateSecs : r.previousSecs;
  const when = current ? c.sameStage : "";
  if (prev === 0) return c.nothing(w.previous, when);
  const pct = Math.round(((r.totalSecs - prev) / prev) * 100);
  if (pct === 0) return c.same(w.previous, when);
  return c.change(`${pct > 0 ? "+" : "−"}${formatNumber(Math.abs(pct))}`, w.versus, when);
}

/**
 * Profil de joueur tiré du récap : chaque profil a un seuil, le score est la donnée divisée par ce
 * seuil (1 = seuil tout juste atteint) ; le plus haut score l'emporte, l'ordre de la liste départage.
 * Aucun seuil atteint : « Le joueur tranquille ».
 */
export function playerProfile(r: Recap, title: (gameId: string) => string): Profile {
  const w = words(r.period);
  const s = THRESHOLDS[r.period];
  const p = t.activity.profile;
  const when = r.back === 0 ? w.current : w.past;
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
      profile: { id: "night", title: p.night.title, line: p.night.line(percent(r.momentSecs[0], moments)) },
    },
    {
      score: moments > 0 ? r.momentSecs[1] / moments / 0.45 : 0,
      profile: { id: "early", title: p.early.title, line: p.early.line(percent(r.momentSecs[1], moments)) },
    },
    {
      score: Math.max(average / 7200, longest / 14400),
      profile: { id: "marathon", title: p.marathon.title, line: p.marathon.line(formatDuration(average || 60), formatDuration(longest || 60)) },
    },
    {
      score: news >= 0.4 * Math.max(1, r.games) ? news / s.explorer : 0,
      profile: { id: "explorer", title: p.explorer.title, line: p.explorer.line(news, when) },
    },
    {
      score: r.achievements / s.hunter,
      profile: { id: "hunter", title: p.hunter.title, line: p.hunter.line(r.achievements, when) },
    },
    {
      score: r.bestStreak / s.regular,
      profile: { id: "regular", title: p.regular.title, line: p.regular.line(r.bestStreak) },
    },
    {
      score: top && r.sessions >= 3 && r.games >= 1 ? topShare / 0.7 : 0,
      profile: { id: "loyal", title: p.loyal.title, line: top ? p.loyal.line(percent(top.secs, r.totalSecs), title(top.gameId)) : "" },
    },
    {
      score: r.sessions >= 3 && weekdays > 0 ? weekend / weekdays / 0.6 : 0,
      profile: { id: "weekend", title: p.weekend.title, line: p.weekend.line(percent(weekend, weekdays)) },
    },
    {
      score: topShare < 0.4 ? r.games / s.zapper : 0,
      profile: { id: "zapper", title: p.zapper.title, line: p.zapper.line(r.games) },
    },
  ];
  let best: { profile: Profile; score: number } | null = null;
  for (const c of candidates) {
    if (c.score >= 1 && (!best || c.score > best.score)) best = c;
  }
  return best?.profile ?? { id: "casual", title: p.casual.title, line: p.casual.line(r.sessions) };
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
  const a = t.activity;
  const s = a.story;
  const w = words(r.period);
  const when = r.back === 0 ? w.current : w.past;
  const game = (id: string | undefined | null) => (id && ctx.known(id) ? id : null);
  const first = r.top[0]?.gameId ?? null;
  const pages: StoryPage[] = [];

  pages.push(
    page("total", {
      gameId: game(first),
      eyebrow: periodTitle(r),
      value: formatDuration(r.totalSecs),
      title: s.totalTitle(when),
      detail: storyComparison(r),
    }),
  );

  if (r.top.length > 0) {
    const max = Math.max(1, r.top[0].secs);
    pages.push(
      page("top", {
        gameId: game(first),
        eyebrow: s.topEyebrow,
        title: r.top.length === 1 ? s.topOne(when) : s.topMany(r.top.length),
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
        eyebrow: s.rhythmEyebrow,
        value: capitalize(a.weekdays[r.favoriteWeekday]),
        title: s.rhythmTitle,
        detail: moment === null ? null : s.rhythmDetail(a.moments[moment], a.momentSpan[moment]),
        bars: r.weekdaySecs.map((secs, i) => ({ label: a.weekdayLetters[i], share: secs / max, active: i === r.favoriteWeekday })),
      }),
    );
  }

  if (r.longest && r.sessions >= 2) {
    const start = new Date(r.longest.start * 1000);
    pages.push(
      page("longest", {
        gameId: game(r.longest.gameId),
        eyebrow: s.longestEyebrow,
        value: formatDuration(r.longest.secs),
        title: ctx.title(r.longest.gameId),
        detail: s.longestDetail(capitalize(dayFull().format(start)), hourMinute().format(start)),
        logo: true,
      }),
    );
  }

  if (r.bestStreak >= 2) {
    const running = r.back === 0 && r.currentStreak >= 2;
    pages.push(
      page("streak", {
        gameId: game(r.top[2]?.gameId ?? first),
        eyebrow: s.streakEyebrow,
        value: a.days(r.bestStreak),
        title: s.streakTitle,
        detail: running ? s.streakRunning(a.days(r.currentStreak)) : s.streakLongest(when),
      }),
    );
  }

  if (r.newGames.length > 0) {
    const n = r.newGames.length;
    pages.push(
      page("discoveries", {
        gameId: game(r.newGames[0].gameId),
        eyebrow: s.newEyebrow,
        value: formatNumber(n),
        title: s.newTitle(n),
        detail: n > 5 ? s.newMore(n - 5) : null,
        items: r.newGames.slice(0, 5).map((g) => ({ gameId: g.gameId, label: ctx.title(g.gameId), value: dayMonth().format(new Date(g.firstPlayed * 1000)), share: 0 })),
      }),
    );
  }

  if (r.achievements > 0) {
    const most = r.achievementGames[0];
    const rare = ctx.rarest;
    pages.push(
      page("achievements", {
        gameId: game(rare?.gameId ?? most?.gameId),
        eyebrow: s.achEyebrow,
        value: formatNumber(r.achievements),
        title: s.achTitle(r.achievements),
        detail: most && r.achievementGames.length > 1 ? s.achSome(most.count, ctx.title(most.gameId)) : most ? s.achIn(ctx.title(most.gameId)) : null,
        achievement: rare,
      }),
    );
  }

  const profile = playerProfile(r, ctx.title);
  pages.push(
    page("profile", {
      gameId: game(r.top[3]?.gameId ?? first),
      eyebrow: s.profileEyebrow,
      title: profile.title,
      detail: profile.line,
    }),
  );

  pages.push(
    page("end", {
      gameId: game(first),
      eyebrow: periodTitle(r),
      value: formatDuration(r.totalSecs),
      title: r.back === 0 ? w.next : s.endPast,
      detail: `${a.sessions(r.sessions)} · ${a.games(r.games)}`,
    }),
  );

  return pages.slice(0, MAX_PAGES);
}

/** « Débloqué par 1,2 % des joueurs Steam » (ou GOG : pourcentage de la boutique du succès). */
export const rarityText = (percent: number, store: AchievementStore) =>
  t.activity.story.rarity(formatNumber(percent < 0.1 ? 0.1 : percent, { maximumFractionDigits: 1 }), percent < 0.1, PLATFORM_LABEL[store]);

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
