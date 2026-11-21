// Gestion du focus partagée (accessibilité) : retour du focus après un champ sur place, piège à
// focus des dialogues modaux. Touche au DOM, donc hors de `src/lib/` (la logique pure est dans a11y.ts).
import { trapNext } from "../lib/a11y";

/** Élément qui a le focus maintenant (pas `body`), à garder pour le lui rendre plus tard. */
export function currentFocus(): HTMLElement | null {
  const el = document.activeElement;
  return el instanceof HTMLElement && el !== document.body ? el : null;
}

const FOCUSABLE = 'button:not(:disabled), [href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), video[controls], [tabindex]:not([tabindex="-1"])';

/** Élément qui a le focus et son chemin d'index depuis `body`, relevés à l'ouverture d'un champ. */
export type FocusMark = { el: HTMLElement; path: number[] } | null;

/**
 * Relève l'élément qui a le focus (à appeler à l'ouverture du champ, quand le bouton est encore
 * dans le DOM : il en sort souvent juste après, remplacé par le champ).
 */
export function markFocus(): FocusMark {
  const el = currentFocus();
  if (!el) return null;
  const path: number[] = [];
  for (let at: Element = el; at.parentElement && at !== document.body; at = at.parentElement) {
    path.unshift([...at.parentElement.children].indexOf(at));
  }
  return { el, path };
}

/**
 * Rend le focus à l'élément relevé par `markFocus` après la fermeture d'un champ sur place (Entrée
 * ou Échap), une fois que React a retiré le champ (`setTimeout` plutôt que `requestAnimationFrame`,
 * freiné quand la fenêtre est en arrière-plan). S'il a disparu (bouton remplacé par le champ puis
 * recréé), l'élément revenu au même endroit du DOM le reçoit. Rien si le focus est déjà ailleurs.
 */
export function restoreFocus(mark: FocusMark) {
  if (!mark) return;
  window.setTimeout(() => {
    if (currentFocus()) return;
    if (mark.el.isConnected) return mark.el.focus();
    let back: Element | undefined = document.body;
    for (const i of mark.path) back = back?.children[i];
    const target = back instanceof HTMLElement && back.matches(FOCUSABLE) ? back : back?.querySelector<HTMLElement>(FOCUSABLE);
    target?.focus();
  }, 0);
}

/**
 * Tab et Maj+Tab dans un dialogue modal : le focus boucle sur ses éléments au lieu de passer sous
 * le dialogue (la page derrière reste atteignable à la souris seulement). Renvoie vrai si la touche
 * a été prise.
 */
export function trapTab(e: KeyboardEvent, root: HTMLElement | null): boolean {
  if (e.key !== "Tab" || !root) return false;
  const items = [...root.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.getClientRects().length > 0);
  const next = trapNext(items.length, items.indexOf(document.activeElement as HTMLElement), e.shiftKey);
  e.preventDefault();
  if (next !== null) items[next].focus();
  return true;
}
