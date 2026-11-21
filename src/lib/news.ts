import type { News, NewsAlert } from "./api";
import { t } from "./i18n";

/** Annonce publiée depuis la dernière partie : « Nouveau ». Jamais joué : rien n'est marqué. */
export function isNewNews(news: Pick<News, "date">, lastPlayed: number | null): boolean {
  return lastPlayed !== null && lastPlayed > 0 && news.date > lastPlayed;
}

/** Nombre d'annonces publiées depuis la dernière partie. */
export function newNewsCount(list: readonly Pick<News, "date">[], lastPlayed: number | null): number {
  return list.filter((n) => isNewNews(n, lastPlayed)).length;
}

/** « Rocket League et Terraria », « Rocket League, Terraria et 2 autres ». */
function gameList(names: string[]): string {
  const and = ` ${t.fiche.news.and} `;
  if (names.length <= 2) return names.join(and);
  return `${names.slice(0, 2).join(", ")}${and}${t.fiche.news.others(names.length - 2)}`;
}

/** Texte du toast d'annonces récentes des jeux « En cours » ; `null` sans annonce. */
export function newsToast(alerts: readonly NewsAlert[]): { title: string; detail: string } | null {
  if (alerts.length === 0) return null;
  const n = t.fiche.news;
  if (alerts.length === 1) {
    const a = alerts[0];
    return { title: a.update ? n.toastUpdate(a.game) : n.toastNews(a.game), detail: a.title };
  }
  const games = [...new Set(alerts.map((a) => a.game))];
  return { title: n.toastMany(alerts.length), detail: gameList(games) };
}
