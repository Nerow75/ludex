import type { Game } from "./api";

/** Période de l'Historique, de la plus récente à la plus ancienne. */
export interface HistoryGroup {
  label: string;
  games: Game[];
}

const DAY = 86_400;

/**
 * Regroupe des jeux déjà triés du plus récent au plus ancien (`lastPlayed`, secondes Unix) par
 * période : Aujourd'hui, Hier (jours au fuseau local), puis 7 et 30 derniers jours, 12 derniers mois,
 * plus ancien. Les périodes vides sont absentes ; l'ordre des jeux est gardé.
 */
export function historyGroups(games: readonly Game[], now = Date.now()): HistoryGroup[] {
  const midnight = new Date(now);
  midnight.setHours(0, 0, 0, 0);
  const today = Math.floor(midnight.getTime() / 1000);
  const yesterday = new Date(midnight);
  yesterday.setDate(yesterday.getDate() - 1);
  const nowSecs = Math.floor(now / 1000);
  const bounds: [string, number][] = [
    ["Aujourd'hui", today],
    ["Hier", Math.floor(yesterday.getTime() / 1000)],
    ["7 derniers jours", nowSecs - 7 * DAY],
    ["30 derniers jours", nowSecs - 30 * DAY],
    ["12 derniers mois", nowSecs - 365 * DAY],
    ["Plus ancien", -Infinity],
  ];
  const groups: HistoryGroup[] = [];
  for (const game of games) {
    const label = bounds.find(([, from]) => (game.lastPlayed ?? 0) >= from)![0];
    const last = groups[groups.length - 1];
    if (last?.label === label) last.games.push(game);
    else groups.push({ label, games: [game] });
  }
  return groups;
}
