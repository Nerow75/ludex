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

export function formatSize(bytes: number | null): string {
  if (!bytes) return "Inconnue";
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1).replace(".", ",")} Go`;
  return `${Math.max(1, Math.round(bytes / 1e6))} Mo`;
}

/** Année de sortie (horodatage Unix en secondes), `null` si inconnue. */
export const releaseYear = (ts: number | null): number | null => (ts ? new Date(ts * 1000).getFullYear() : null);

const fullDate = new Intl.DateTimeFormat("fr-FR", { day: "numeric", month: "long", year: "numeric" });

/** « 10 décembre 2020 ». */
export const formatDate = (ts: number) => fullDate.format(new Date(ts * 1000));
