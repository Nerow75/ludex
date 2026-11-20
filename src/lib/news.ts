import type { News, NewsAlert } from "./api";

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
  if (names.length <= 2) return names.join(" et ");
  const rest = names.length - 2;
  return `${names.slice(0, 2).join(", ")} et ${rest} autre${rest > 1 ? "s" : ""}`;
}

/** Texte du toast d'annonces récentes des jeux « En cours » ; `null` sans annonce. */
export function newsToast(alerts: readonly NewsAlert[]): { title: string; detail: string } | null {
  if (alerts.length === 0) return null;
  if (alerts.length === 1) {
    const a = alerts[0];
    return { title: a.update ? `Mise à jour de ${a.game}` : `Nouvelle annonce pour ${a.game}`, detail: a.title };
  }
  const games = [...new Set(alerts.map((a) => a.game))];
  return { title: `Actualités de ${alerts.length} jeux en cours`, detail: gameList(games) };
}
