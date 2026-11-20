import type { NotificationKind } from "./api";

/** Lignes de la section « Notifications » des Paramètres, dans l'ordre affiché. */
export const NOTIFICATION_ROWS: readonly { kind: NotificationKind; label: string; detail: string }[] = [
  { kind: "download", label: "Téléchargement terminé", detail: "Un jeu Steam installé ou mis à jour est prêt." },
  { kind: "news", label: "Mise à jour d'un jeu en cours", detail: "Annonce récente d'un jeu au statut « En cours », quand Ludex est caché." },
  { kind: "lowDisk", label: "Disque presque plein", detail: "Moins de 10 % libres sur un disque de jeux, une fois par jour au plus." },
  { kind: "update", label: "Nouvelle version de Ludex", detail: "Une mise à jour de Ludex est prête à installer." },
  { kind: "savesError", label: "Copie des sauvegardes ratée", detail: "La copie automatique de fin de partie a échoué." },
];
