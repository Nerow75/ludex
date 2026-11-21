// Textes de l'export et de l'import des données perso (section « Données » des Paramètres).
import type { ImportPreview, TransferSummary } from "./api";
import { formatDate } from "./format";
import { t } from "./i18n";

/** Éléments d'un résumé, dans l'ordre d'importance. */
const PARTS = [
  "favorites",
  "collections",
  "statuses",
  "notes",
  "playtimeGames",
  "sessions",
  "manualGames",
  "titles",
  "aliases",
  "genres",
  "launchArgs",
  "wishes",
  "covers",
  "hidden",
] as const satisfies readonly (keyof TransferSummary)[];

/** Éléments non vides d'un export, dans l'ordre d'importance : « 12 favoris », « 3 collections »… */
export function summaryParts(s: TransferSummary): string[] {
  const text = t.settings.transfer;
  return PARTS.filter((key) => s[key] > 0).map((key) => text[key](s[key]));
}

/** Contenu en une phrase : les quatre premiers éléments, puis « et N autres ». */
export function summaryText(s: TransferSummary): string {
  const text = t.settings.transfer;
  const parts = summaryParts(s);
  if (!parts.length) return text.settingsOnly;
  if (parts.length <= 4) return parts.length === 1 ? parts[0] : text.and(parts.slice(0, -1).join(", "), parts[parts.length - 1]);
  return text.more(parts.slice(0, 4).join(", "), parts.length - 4);
}

/** « Export de Ludex 0.2.0 du 19 novembre 2026 » (« Ludex 0.2.0 export from Nov 19, 2026 »). */
export function previewOrigin(p: ImportPreview): string {
  return t.settings.transfer.origin(p.ludexVersion, p.created > 0 ? formatDate(p.created) : "");
}
