// Textes de l'export et de l'import des données perso (section « Données » des Paramètres).
import type { ImportPreview, TransferSummary } from "./api";
import { formatDate } from "./format";

const count = (n: number, one: string, many: string) => `${n.toLocaleString("fr-FR")} ${n > 1 ? many : one}`;

/** Éléments non vides d'un export, dans l'ordre d'importance : « 12 favoris », « 3 collections »… */
export function summaryParts(s: TransferSummary): string[] {
  const parts: [number, string, string][] = [
    [s.favorites, "favori", "favoris"],
    [s.collections, "collection", "collections"],
    [s.statuses, "statut", "statuts"],
    [s.notes, "note", "notes"],
    [s.playtimeGames, "temps de jeu mesuré", "temps de jeu mesurés"],
    [s.sessions, "session", "sessions"],
    [s.manualGames, "jeu ajouté à la main", "jeux ajoutés à la main"],
    [s.titles, "titre choisi", "titres choisis"],
    [s.aliases, "jeu avec des alias", "jeux avec des alias"],
    [s.genres, "jeu aux genres corrigés", "jeux aux genres corrigés"],
    [s.launchArgs, "option de lancement", "options de lancement"],
    [s.covers, "image perso", "images perso"],
    [s.hidden, "jeu masqué", "jeux masqués"],
  ];
  return parts.filter(([n]) => n > 0).map(([n, one, many]) => count(n, one, many));
}

/** Contenu en une phrase : les quatre premiers éléments, puis « et N autres ». */
export function summaryText(s: TransferSummary): string {
  const parts = summaryParts(s);
  if (!parts.length) return "réglages seulement";
  if (parts.length <= 4) return parts.length === 1 ? parts[0] : `${parts.slice(0, -1).join(", ")} et ${parts[parts.length - 1]}`;
  return `${parts.slice(0, 4).join(", ")} et ${parts.length - 4} autre${parts.length - 4 > 1 ? "s" : ""}`;
}

/** « Export de Ludex 0.2.0 du 19 novembre 2026 ». */
export function previewOrigin(p: ImportPreview): string {
  const version = p.ludexVersion ? ` ${p.ludexVersion}` : "";
  const date = p.created > 0 ? ` du ${formatDate(p.created)}` : "";
  return `Export de Ludex${version}${date}`;
}
