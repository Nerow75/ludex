// Résumé du premier scan pour l'écran de premier lancement : fonctions pures.
import { PLATFORM_LABEL, type Game, type Platform, type ScanResult } from "./api";

export interface SourceCount {
  platform: Platform;
  label: string;
  count: number;
}

export interface WelcomeSummary {
  /** Jeux visibles (outils masqués exclus). */
  total: number;
  /** « trouvés dans Steam, tes dossiers de jeux et 5 autres ». Vide sans jeu. */
  where: string;
  /** Sources qui ont des jeux visibles, la plus fournie d'abord. */
  found: SourceCount[];
  /** Launchers installés sans jeu. */
  empty: string[];
  /** Launchers absents de ce PC. */
  missing: string[];
  /** Launchers présents mais illisibles. */
  failed: string[];
  /** Applis (Wallpaper Engine…) masquées d'office. */
  tools: number;
}

/** « Autres » dans une phrase : les jeux trouvés dans les dossiers de jeux. */
const SOURCE_LABEL: Record<Platform, string> = { ...PLATFORM_LABEL, other: "Dossiers de jeux" };
const IN_SENTENCE: Record<Platform, string> = { ...PLATFORM_LABEL, other: "tes dossiers de jeux" };

const byLabel = (a: string, b: string) => a.localeCompare(b, "fr");

/** « A », « A et B », « A, B et C », au-delà « A, B et 4 autres ». */
export function sourceList(names: string[]): string {
  if (names.length <= 1) return names[0] ?? "";
  if (names.length <= 3) return `${names.slice(0, -1).join(", ")} et ${names[names.length - 1]}`;
  return `${names[0]}, ${names[1]} et ${names.length - 2} autres`;
}

export function summarize({ games, sources }: Pick<ScanResult, "games" | "sources">): WelcomeSummary {
  const visible = games.filter((g: Game) => !g.hidden);
  const counts = new Map<Platform, number>();
  visible.forEach((g) => counts.set(g.platform, (counts.get(g.platform) ?? 0) + 1));
  const found = [...counts]
    .map(([platform, count]) => ({ platform, label: SOURCE_LABEL[platform], count }))
    .sort((a, b) => b.count - a.count || byLabel(a.label, b.label));
  const launchers = sources.filter((s) => s.platform !== "other");
  const names = (list: typeof sources) => list.map((s) => PLATFORM_LABEL[s.platform]).sort(byLabel);
  return {
    total: visible.length,
    where: found.length ? `${visible.length === 1 ? "trouvé" : "trouvés"} dans ${sourceList(found.map((s) => IN_SENTENCE[s.platform]))}` : "",
    found,
    // Un launcher dont tous les jeux sont masqués (outils) n'est pas « sans jeu ».
    empty: names(launchers.filter((s) => s.installed && !s.error && s.count === 0)),
    missing: names(launchers.filter((s) => !s.installed)),
    failed: names(launchers.filter((s) => s.error)),
    tools: games.filter((g) => g.isTool && g.hidden).length,
  };
}
