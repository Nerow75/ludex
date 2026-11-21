// Résumé du premier scan pour l'écran de premier lancement : fonctions pures.
import { PLATFORM_LABEL, type Game, type Platform, type ScanResult } from "./api";
import { live, locale, t } from "./i18n";

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

/** « Autres » dans une liste et dans une phrase : les jeux trouvés dans les dossiers de jeux (lus à l'appel, dans la langue actuelle). */
const sourceLabel = (p: Platform) => (p === "other" ? t.settings.welcome.otherSource : PLATFORM_LABEL[p]);
const inSentence = (p: Platform) => (p === "other" ? t.settings.welcome.otherInSentence : PLATFORM_LABEL[p]);

const byLabel = (a: string, b: string) => a.localeCompare(b, locale());

/** « A », « A et B », « A, B et C », au-delà « A, B et 4 autres ». */
export function sourceList(names: string[]): string {
  const w = t.settings.welcome;
  if (names.length <= 1) return names[0] ?? "";
  if (names.length === 2) return w.listTwo(names[0], names[1]);
  if (names.length === 3) return w.listThree(names[0], names[1], names[2]);
  return w.listMore(names[0], names[1], names.length - 2);
}

export function summarize({ games, sources }: Pick<ScanResult, "games" | "sources">): WelcomeSummary {
  const visible = games.filter((g: Game) => !g.hidden);
  const counts = new Map<Platform, number>();
  visible.forEach((g) => counts.set(g.platform, (counts.get(g.platform) ?? 0) + 1));
  const found = [...counts]
    .map(([platform, count]) => ({ platform, label: sourceLabel(platform), count }))
    .sort((a, b) => b.count - a.count || byLabel(a.label, b.label));
  const launchers = sources.filter((s) => s.platform !== "other");
  const names = (list: typeof sources) => list.map((s) => PLATFORM_LABEL[s.platform]).sort(byLabel);
  return {
    total: visible.length,
    where: found.length ? t.settings.welcome.where(visible.length, sourceList(found.map((s) => inSentence(s.platform)))) : "",
    found,
    // Un launcher dont tous les jeux sont masqués (outils) n'est pas « sans jeu ».
    empty: names(launchers.filter((s) => s.installed && !s.error && s.count === 0)),
    missing: names(launchers.filter((s) => !s.installed)),
    failed: names(launchers.filter((s) => s.error)),
    tools: games.filter((g) => g.isTool && g.hidden).length,
  };
}

/** Étapes de l'accueil, dans l'ordre (noms lus par les lecteurs d'écran, dans la langue actuelle). */
export const STEPS: readonly string[] = live(() => t.settings.welcome.steps);

/** Étape suivante (`1`) ou précédente (`-1`) ; `null` au-delà des bords (après la dernière : fin). */
export function stepAfter(step: number, delta: 1 | -1): number | null {
  const to = step + delta;
  return to >= 0 && to < STEPS.length ? to : null;
}
