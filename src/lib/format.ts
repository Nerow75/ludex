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

/** « 45 min », « 2,5 h », « 12 h », « 1 240 h ». `null` : aucun chiffre connu. */
export function formatPlaytime(minutes: number | null): string {
  if (minutes === null) return "Non mesuré";
  if (minutes < 1) return "Jamais joué";
  if (minutes < 60) return `${Math.round(minutes)} min`;
  const hours = minutes / 60;
  return `${(hours < 10 ? hoursFine : hoursRound).format(hours)} h`;
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
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1).replace(".", ",")} Go`;
  return `${Math.max(1, Math.round(bytes / 1e6))} Mo`;
}
