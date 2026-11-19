import type { InstallState } from "./api";

/** Comparaison sans accents ni casse, pour la recherche. */
export const norm = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

/** Titre sans ® ni ™, pour l'affichage compact et le tri. */
export const cleanTitle = (s: string) => s.replace(/[®™]/g, "").trim();

const now = () => Math.floor(Date.now() / 1000);

/** « il y a 3 jours », « hier »… à partir d'un horodatage Unix en secondes. */
export function relativeTime(ts: number | null): string {
  if (!ts) return "Jamais lancé";
  const d = Math.max(0, now() - ts);
  const days = Math.floor(d / 86400);
  if (d < 60) return "à l'instant";
  if (d < 3600) return `il y a ${Math.round(d / 60)} min`;
  if (d < 86400) return `il y a ${Math.round(d / 3600)} h`;
  if (days < 2) return "hier";
  if (days < 30) return `il y a ${days} jours`;
  const months = Math.round(days / 30.4);
  return months < 12 ? `il y a ${months} mois` : "il y a plus d'un an";
}

const hoursFine = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 1 });
const hoursRound = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 0 });
const integer = new Intl.NumberFormat("fr-FR");

/** « 45 min », « 2,5 h », « 12 h », « 1 240 h ». `null` : aucun chiffre connu. */
export function formatPlaytime(minutes: number | null): string {
  if (minutes === null) return "Non mesuré";
  if (minutes < 1) return "Jamais joué";
  if (minutes < 60) return `${Math.round(minutes)} min`;
  const hours = minutes / 60;
  return `${(hours < 10 ? hoursFine : hoursRound).format(hours)} h`;
}

/** « 34 / 52 », « 1 328 / 1 328 ». */
export function formatAchievements(unlocked: number, total: number): string {
  return `${integer.format(unlocked)} / ${integer.format(total)}`;
}

/** Durée d'une session : « 45 min », « 1 h 20 », « 3 h ». */
export function formatDuration(secs: number): string {
  const minutes = Math.round(secs / 60);
  if (minutes < 60) return `${Math.max(1, minutes)} min`;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return m === 0 ? `${h} h` : `${h} h ${String(m).padStart(2, "0")}`;
}

export function formatSize(bytes: number | null): string {
  if (!bytes) return "Inconnue";
  if (bytes >= 1e12) return `${(bytes / 1e12).toFixed(1).replace(".", ",")} To`;
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1).replace(".", ",")} Go`;
  return `${Math.max(1, Math.round(bytes / 1e6))} Mo`;
}

/** Année de sortie (horodatage Unix en secondes), `null` si inconnue. */
export const releaseYear = (ts: number | null): number | null => (ts ? new Date(ts * 1000).getFullYear() : null);

const fullDate = new Intl.DateTimeFormat("fr-FR", { day: "numeric", month: "long", year: "numeric" });

const percent = (progress: number | null) => (progress === null ? "" : ` ${progress} %`);

/** Mention courte de la carte : « Mise à jour requise », « Téléchargement 42 % »… `null` si le jeu est prêt. */
export function installBadge(state: InstallState): string | null {
  switch (state.kind) {
    case "ready": return null;
    case "update_required": return "Mise à jour requise";
    case "queued": return "En file d'attente";
    case "updating": return `Téléchargement${percent(state.progress)}`;
    case "paused": return `En pause${percent(state.progress)}`;
    case "incomplete": return "Installation incomplète";
  }
}

/** Mention de la fiche, près du bouton Jouer : ce que fera le launcher. `null` si le jeu est prêt. */
export function installNote(state: InstallState, launcher: string): string | null {
  switch (state.kind) {
    case "ready": return null;
    case "update_required": return `Mise à jour requise : ${launcher} la fera au lancement`;
    case "queued": return `Mise à jour en file d'attente chez ${launcher}`;
    case "updating": return state.progress === null ? "Téléchargement en cours" : `Téléchargement ${state.progress} %`;
    case "paused": return `Mise à jour en pause${percent(state.progress)} : ${launcher} la reprendra au lancement`;
    case "incomplete": return `Installation incomplète : ${launcher} la terminera au lancement`;
  }
}

/** « 10 décembre 2020 ». */
export const formatDate = (ts: number) => fullDate.format(new Date(ts * 1000));
