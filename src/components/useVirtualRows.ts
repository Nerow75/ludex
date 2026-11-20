import { useCallback, useEffect, useLayoutEffect, useRef, useState, type RefObject } from "react";
import { flushSync } from "react-dom";
import { plan, rowsBetween, rowTop, type Rows, type Segment } from "../lib/virtual";

interface Options {
  /** Virtualisation active (au-delà de `VIRTUAL_MIN` jeux) ; sinon `null` : tout est rendu. */
  enabled: boolean;
  /** Nombre de rangées (grille) ou de lignes (liste). */
  count: number;
  /** Rangée toujours rendue (jeu choisi : son focus, sa fiche, le défilement vers lui), -1 : aucune. */
  keep: number;
  /** Rangée sous laquelle la fiche est dépliée, -1 : aucune. */
  extraRow: number;
  /** Hauteur estimée d'une rangée avant la première mesure. */
  estimate: number;
  /** Sélecteur d'une rangée rendue, enfant direct du conteneur (`:scope > .card`). */
  item: string;
}

/** Ancêtre qui défile (`.scroll` de l'app), sinon la page. */
function scrollParent(el: HTMLElement): HTMLElement {
  for (let p = el.parentElement; p; p = p.parentElement) {
    if (/(auto|scroll)/.test(getComputedStyle(p).overflowY)) return p;
  }
  return document.scrollingElement as HTMLElement;
}

const near = (a: number, b: number) => Math.abs(a - b) < 0.5;

/**
 * Rangées à rendre d'une grille ou d'une liste longue : celles qui touchent l'écran plus une hauteur
 * d'écran de marge de chaque côté, et la rangée `keep`. Hauteurs mesurées sur le DOM (première rangée
 * rendue, écart du conteneur, fiche marges comprises), relues à chaque rendu et quand le conteneur change
 * de taille. Le plan suit le défilement : nouveau rendu seulement quand les rangées changent, fait avant
 * l'affichage de l'image suivante pour ne jamais montrer de vide.
 */
export function useVirtualRows(ref: RefObject<HTMLElement | null>, o: Options): Segment[] | null {
  const [range, setRange] = useState<[number, number]>([0, 0]);
  const [size, setSize] = useState(0);
  const [gap, setGap] = useState(0);
  const [extraSize, setExtraSize] = useState(0);
  /** Haut de la rangée 0 dans le contenu de l'ancêtre qui défile. */
  const origin = useRef(0);
  const scroller = useRef<HTMLElement | null>(null);

  const rows: Rows = {
    count: o.count,
    size: size || o.estimate,
    gap,
    extra: o.extraRow >= 0 && o.extraRow < o.count ? { row: o.extraRow, size: extraSize } : null,
  };
  const segments = o.enabled ? plan(rows, range, o.keep >= 0 ? [o.keep] : []) : null;
  const live = useRef({ rows, segments, o, extraSize });
  live.current = { rows, segments, o, extraSize };

  /** Rangées proches de l'écran ; `sync` : rendu immédiat (défilement), avant que l'image soit peinte. */
  const update = useCallback((sync: boolean) => {
    const el = scroller.current;
    const { rows, o } = live.current;
    if (!el || !o.enabled) return;
    const top = el.scrollTop - origin.current;
    const view = el.clientHeight;
    const next = rowsBetween(rows, top - view, top + 2 * view);
    const apply = () => setRange((r) => (r[0] === next[0] && r[1] === next[1] ? r : next));
    if (sync) flushSync(apply);
    else apply();
  }, []);

  /** Relit les hauteurs et la position de la rangée 0, puis les rangées à rendre. */
  const measure = useCallback(() => {
    const box = ref.current;
    const { rows, segments, o, extraSize } = live.current;
    if (!box || !o.enabled) return;
    const el = (scroller.current ??= scrollParent(box));
    const first = box.querySelector<HTMLElement>(o.item);
    // Hauteurs fractionnaires (largeur de colonne × 1,5) : `offsetHeight` arrondi dériverait sur des centaines de rangées.
    const h = first ? first.getBoundingClientRect().height : 0;
    if (h && Math.abs(h - rows.size) > 0.01) setSize(h);
    const g = parseFloat(getComputedStyle(box).rowGap) || 0;
    if (!near(g, rows.gap)) setGap(g);
    const strip = box.querySelector<HTMLElement>(":scope > .strip");
    if (strip) {
      const cs = getComputedStyle(strip);
      const e = strip.getBoundingClientRect().height + (parseFloat(cs.marginTop) || 0) + (parseFloat(cs.marginBottom) || 0);
      if (!near(e, extraSize)) setExtraSize(e);
    }
    // La première rangée rendue est toujours avant la fiche (la rangée de la fiche est gardée) :
    // sa position ne dépend donc pas de la hauteur de la fiche.
    const firstRow = segments?.find((s) => s.kind === "rows");
    if (first && firstRow?.kind === "rows") {
      const y = first.getBoundingClientRect().top - el.getBoundingClientRect().top + el.scrollTop;
      origin.current = y - rowTop({ ...rows, size: h || rows.size, gap: g }, firstRow.first);
    }
    update(false);
  }, [ref, update]);

  // Après chaque rendu : hauteurs, position, rangées (un écart = nouveau rendu avant l'affichage).
  useLayoutEffect(() => {
    measure();
  });

  // Défilement et changements de taille (fenêtre, fiche qui change d'onglet, colonnes).
  useEffect(() => {
    const box = ref.current;
    if (!o.enabled || !box) return;
    const el = (scroller.current ??= scrollParent(box));
    const onScroll = () => update(true);
    let frame = 0;
    const onResize = () => {
      if (!frame) frame = requestAnimationFrame(() => {
        frame = 0;
        measure();
      });
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    const ro = new ResizeObserver(onResize);
    ro.observe(el);
    ro.observe(box);
    return () => {
      el.removeEventListener("scroll", onScroll);
      ro.disconnect();
      cancelAnimationFrame(frame);
    };
  }, [o.enabled, ref, update, measure]);

  return segments;
}
