import type { Game, InstallState } from "./api";
import { locale, nf, t } from "./i18n";
import { RANK_META, searchRank } from "./search";

/** Comparaison sans accents ni casse, pour la recherche. */
export const norm = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

/** Titre sans ® ni ™, pour l'affichage compact et le tri. */
export const cleanTitle = (s: string) => s.replace(/[®™]/g, "").trim();

/**
 * Recherche (`q` déjà passée par `norm` ou brute) : 0 si le titre correspond, même de loin
 * (préfixe, initiales, sous-chaîne, fautes de frappe : voir `search.ts`), 1 si un studio,
 * un éditeur ou un tag Steam commence par elle (« ubisoft », « rogue », « coop »), `null` sinon.
 * Pour classer les résultats, `searchRank`.
 */
export function queryMatch(game: Game, q: string): 0 | 1 | null {
  const r = searchRank(game, q);
  return r === null ? null : r < RANK_META ? 0 : 1;
}

const now = () => Math.floor(Date.now() / 1000);

/**
 * « il y a 3 jours », « hier »… (« 3 days ago », « yesterday » en anglais) à partir d'un
 * horodatage Unix en secondes.
 */
export function relativeTime(ts: number | null): string {
  const ago = t.common.ago;
  if (!ts) return ago.never;
  const d = Math.max(0, now() - ts);
  const days = Math.floor(d / 86400);
  if (d < 60) return ago.justNow;
  // Arrondi plafonné : 59 min 50 s reste « 59 min », pas « 60 min ».
  if (d < 3600) return ago.minutes(Math.min(59, Math.round(d / 60)));
  if (d < 86400) return ago.hours(Math.min(23, Math.round(d / 3600)));
  if (days < 2) return ago.yesterday;
  if (days < 30) return ago.days(days);
  // Moins d'un an : 11 mois au plus (350 jours n'est pas « plus d'un an »).
  return days < 365 ? ago.months(Math.min(11, Math.round(days / 30.4))) : ago.overAYear;
}

/** « 45 min », « 2,5 h », « 12 h », « 1 240 h » (« 2.5 h », « 1,240 h » en anglais). `null` : aucun chiffre connu. */
export function formatPlaytime(minutes: number | null): string {
  const time = t.common.time;
  if (minutes === null) return time.notTracked;
  if (minutes < 1) return time.neverPlayed;
  if (minutes < 60) return time.minutes(String(Math.round(minutes)));
  const hours = minutes / 60;
  return time.hours(nf({ maximumFractionDigits: hours < 10 ? 1 : 0 }).format(hours));
}

/** « 34 / 52 », « 1 328 / 1 328 » (« 1,328 / 1,328 » en anglais). */
export function formatAchievements(unlocked: number, total: number): string {
  return `${nf().format(unlocked)} / ${nf().format(total)}`;
}

/** Part des joueurs : « 0,3 % », « 12 % », « moins de 0,1 % » (« 0.3% », « less than 0.1% »). */
export function formatPercent(p: number): string {
  const percent = t.common.percent;
  if (p < 0.1) return percent.under(nf().format(0.1));
  return percent.value(nf({ maximumFractionDigits: p < 10 ? 1 : 0 }).format(p));
}

/** Durée d'une session : « 45 min », « 1 h 20 », « 3 h » (« 1h 20m », « 3h » en anglais). */
export function formatDuration(secs: number): string {
  const minutes = Math.round(secs / 60);
  if (minutes < 60) return t.common.time.minutes(String(Math.max(1, minutes)));
  return t.common.time.hoursMinutes(Math.floor(minutes / 60), minutes % 60);
}

/** Une décimale (`toFixed`), avec le séparateur décimal de la langue. */
const fixed1 = (n: number) => n.toFixed(1).replace(".", t.common.size.decimal);

/** Taille d'un jeu : « 350 Mo », « 1,5 Go », « 2,3 To » (« 1.5 GB » en anglais), « Inconnue ». */
export function formatSize(bytes: number | null): string {
  const size = t.common.size;
  if (!bytes) return size.unknown;
  if (bytes >= 1e12) return size.tb(fixed1(bytes / 1e12));
  if (bytes >= 1e9) return size.gb(fixed1(bytes / 1e9));
  return size.mb(String(Math.max(1, Math.round(bytes / 1e6))));
}

/** Année de sortie (horodatage Unix en secondes), `null` si inconnue. */
export const releaseYear = (ts: number | null): number | null => (ts ? new Date(ts * 1000).getFullYear() : null);

// Formateurs de dates, gardés par locale et options (en créer un coûte cher).
const dateFormats = new Map<string, Intl.DateTimeFormat>();
function df(options: Intl.DateTimeFormatOptions): Intl.DateTimeFormat {
  const key = `${locale()}|${JSON.stringify(options)}`;
  let format = dateFormats.get(key);
  if (!format) {
    format = new Intl.DateTimeFormat(locale(), options);
    dateFormats.set(key, format);
  }
  return format;
}

const FULL_DATE: Intl.DateTimeFormatOptions = { day: "numeric", month: "long", year: "numeric" };
const CLOCK: Intl.DateTimeFormatOptions = { hour: "2-digit", minute: "2-digit" };

/** Mention courte de la carte : « Mise à jour requise », « Téléchargement 42 % »… `null` si le jeu est prêt. */
export function installBadge(state: InstallState): string | null {
  const install = t.common.install;
  switch (state.kind) {
    case "ready": return null;
    case "update_required": return install.updateRequired;
    case "queued": return install.queued;
    case "updating": return install.updating(state.progress);
    case "paused": return install.paused(state.progress);
    case "incomplete": return install.incomplete;
  }
}

/** Mention de la fiche, près du bouton Jouer : ce que fera le launcher. `null` si le jeu est prêt. */
export function installNote(state: InstallState, launcher: string): string | null {
  const install = t.common.install;
  switch (state.kind) {
    case "ready": return null;
    case "update_required": return install.noteUpdateRequired(launcher);
    case "queued": return install.noteQueued(launcher);
    case "updating": return install.noteUpdating(state.progress);
    case "paused": return install.notePaused(launcher, state.progress);
    case "incomplete": return install.noteIncomplete(launcher);
  }
}

/** « 10 décembre 2020 » (« December 10, 2020 » en anglais). */
export const formatDate = (ts: number) => df(FULL_DATE).format(new Date(ts * 1000));

/** « 10 décembre 2020 à 14:23 » (« December 10, 2020 at 02:23 PM » en anglais). */
export const formatDateTime = (ts: number) => {
  const date = new Date(ts * 1000);
  return t.common.time.dateAt(df(FULL_DATE).format(date), df(CLOCK).format(date));
};

/** Petites tailles comprises : « 12 Ko », « 1,4 Mo », « 2,1 Go » (« 1.4 MB » en anglais). */
export function formatBytes(bytes: number): string {
  const size = t.common.size;
  const oneDecimal = nf({ maximumFractionDigits: 1 });
  if (bytes < 1e3) return size.bytes(String(bytes));
  if (bytes < 1e6) return size.kb(String(Math.round(bytes / 1e3)));
  if (bytes < 1e9) return size.mb(oneDecimal.format(bytes / 1e6));
  return size.gb(oneDecimal.format(bytes / 1e9));
}

/** Jour local d'une date `AAAA-MM-JJ`. */
const dayOf = (date: string) => {
  const [y, m, d] = date.split("-").map(Number);
  return new Date(y, m - 1, d);
};

/**
 * « Aujourd'hui », « Hier », « Il y a 5 jours », « Il y a 3 mois », « Il y a 2 ans » (« Today »,
 * « 5 days ago » en anglais), pour une date `AAAA-MM-JJ`.
 */
export function relativeDay(date: string): string {
  const day = t.common.day;
  const today = new Date();
  const start = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  const days = Math.round((start.getTime() - dayOf(date).getTime()) / 86_400_000);
  if (days <= 0) return day.today;
  if (days === 1) return day.yesterday;
  if (days < 30) return day.days(days);
  if (days < 365) return day.months(Math.max(1, Math.round(days / 30.4)));
  return day.years(Math.floor(days / 365.25));
}

/** « mercredi 18 novembre 2026 » (« Wednesday, November 18, 2026 »), pour une date `AAAA-MM-JJ`. */
export const formatDay = (date: string) => df({ weekday: "long", day: "numeric", month: "long", year: "numeric" }).format(dayOf(date));

/** « 14:05 » (« 02:05 PM » en anglais). */
export const formatClock = (ts: number) => df(CLOCK).format(new Date(ts * 1000));

/** Options de lancement pour une mention courte : coupées à `max` caractères (entre deux options si possible). */
export function shortArgs(text: string, max = 40): string {
  const t = text.trim();
  if (t.length <= max) return t;
  const cut = t.slice(0, max);
  const space = cut.lastIndexOf(" ");
  return `${(space > max / 2 ? cut.slice(0, space) : cut).trimEnd()}…`;
}

/** Exemple d'options pour le champ, selon le launcher. */
export const argsExample = (platform: Game["platform"]) => (platform === "steam" ? "-novid +fps_max 240" : "-windowed -dx11");

/**
 * Nom affiché d'un genre de Ludex. Le nom français (`metadata.rs`) reste l'identifiant partout
 * (corrections, filtre `ludex.genre`) ; un genre inconnu (version future) s'affiche tel quel.
 */
export function genreLabel(name: string): string {
  return Object.prototype.hasOwnProperty.call(t.common.genre, name) ? t.common.genre[name] : name;
}
