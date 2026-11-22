import type { NotificationKind } from "./api";
import { t } from "./i18n";

const KINDS: readonly NotificationKind[] = ["download", "news", "lowDisk", "update", "savesError", "sale", "freeGames"];

/** Lignes de la section « Notifications » des Paramètres, dans l'ordre affiché (libellés lus dans la langue actuelle). */
export const NOTIFICATION_ROWS: readonly { kind: NotificationKind; readonly label: string; readonly detail: string }[] = KINDS.map((kind) => ({
  kind,
  get label() {
    return t.settings.notifications.rows[kind].label;
  },
  get detail() {
    return t.settings.notifications.rows[kind].detail;
  },
}));
