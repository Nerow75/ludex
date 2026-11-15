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

export function formatSize(bytes: number | null): string {
  if (!bytes) return "Inconnue";
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(bytes >= 1e11 ? 0 : 1).replace(".", ",")} Go`;
  return `${Math.max(1, Math.round(bytes / 1e6))} Mo`;
}
