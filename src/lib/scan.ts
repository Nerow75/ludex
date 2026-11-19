// Scan progressif du démarrage : premier affichage partiel (sources rapides), puis complément
// (EA, Battle.net, « Autres ») annoncé par `library-changed`.
import type { LibraryEvent, ScanResult } from "./api";

/** Scan à garder quand un nouveau arrive : un scan complet n'est jamais remplacé par un partiel
 * (le complément peut arriver avant la réponse du premier `scan_library`). */
export function nextScan(current: ScanResult | null, incoming: ScanResult): ScanResult {
  return current && !current.partial && incoming.partial ? current : incoming;
}

/** Bibliothèque encore en lecture : rien de reçu, ou premier affichage sans aucun jeu (les
 * sources lentes peuvent en apporter). Le squelette reste alors affiché plutôt que « Aucun jeu ». */
export function awaitingGames(scan: ScanResult | null): boolean {
  return scan === null || (scan.partial && scan.games.every((g) => g.isTool));
}

/** Écran de premier lancement : après le scan complet seulement (son résumé compte « Autres »). */
export function welcomeReady(scan: ScanResult | null, pending: boolean): boolean {
  return pending && scan !== null && !scan.partial;
}

/** « Nouveau jeu : Hades », « 3 nouveaux jeux », « Jeu retiré : Portal »… `null` si rien n'a changé
 * ou pour le complément du premier scan (ces jeux ne sont pas nouveaux). */
export function libraryToast({ added, removed, complement }: LibraryEvent): { title: string; detail?: string } | null {
  if (complement) return null;
  const list = (titles: string[]) => (titles.length > 1 ? titles.join(", ") : undefined);
  const gone = removed.length === 1 ? `Jeu retiré : ${removed[0]}` : `${removed.length} jeux retirés`;
  if (added.length) {
    const title = added.length === 1 ? `Nouveau jeu : ${added[0]}` : `${added.length} nouveaux jeux`;
    const detail = [list(added), removed.length ? gone : undefined].filter(Boolean).join(" · ");
    return { title, detail: detail || undefined };
  }
  return removed.length ? { title: gone, detail: list(removed) } : null;
}
