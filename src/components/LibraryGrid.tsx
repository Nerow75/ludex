import { Fragment, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import type { Game } from "../lib/api";
import { virtualDisabled } from "../lib/fake";
import { VIRTUAL_MIN } from "../lib/virtual";
import { GameCard } from "./GameCard";
import { useVirtualRows } from "./useVirtualRows";

const CARD_MIN = 140;
const GAP_X = 16;
/** Hauteur du titre et de la ligne sous la jaquette (estimation avant la première mesure). */
const TEXT_H = 46;

/** Nombre de colonnes réel de la grille : même calcul que `repeat(auto-fill, minmax(140px, 1fr))`. */
function useColumns() {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    // Largeur lue tout de suite : le premier affichage a déjà ses vraies colonnes.
    setWidth(el.getBoundingClientRect().width);
    const ro = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const cols = Math.max(1, Math.floor((width + GAP_X) / (CARD_MIN + GAP_X)));
  return { ref, width, cols };
}

interface Props {
  games: Game[];
  selectedId: string | null;
  open: boolean;
  meta: (game: Game) => string;
  /** Jeux dont une session est en cours. */
  playing: ReadonlySet<string>;
  onSelect: (game: Game) => void;
  onPlay: (game: Game) => void;
  /** Fiche à insérer sous la rangée du jeu sélectionné ; reçoit la position de la flèche. */
  renderStrip: (game: Game, notch: number) => ReactNode;
}

/**
 * Grille de jaquettes, fiche insérée sous la rangée du jeu choisi. Au-delà de `VIRTUAL_MIN` jeux, seules
 * les rangées proches de l'écran et celle du jeu choisi sont rendues (`useVirtualRows`), les autres
 * remplacées par des cales `.vgap` : les colonnes calculées ici doivent donc rester celles du CSS.
 */
export function LibraryGrid({ games, selectedId, open, meta, playing, onSelect, onPlay, renderStrip }: Props) {
  const { ref, width, cols } = useColumns();
  const index = games.findIndex((g) => g.id === selectedId);
  const selRow = index < 0 ? -1 : Math.floor(index / cols);
  const rowEnd = index < 0 ? -1 : Math.min(games.length - 1, selRow * cols + cols - 1);
  const cardWidth = (width - (cols - 1) * GAP_X) / cols;
  const notch = (index % cols) * (cardWidth + GAP_X) + cardWidth / 2;
  const showStrip = open && index >= 0 && width > 0;

  const segments = useVirtualRows(ref, {
    enabled: games.length >= VIRTUAL_MIN && !virtualDisabled(),
    count: Math.ceil(games.length / cols),
    keep: selRow,
    extraRow: showStrip ? selRow : -1,
    estimate: Math.max(200, cardWidth * 1.5 + TEXT_H),
    item: ":scope > .card",
  });

  const card = (g: Game) => (
    <GameCard key={g.id} game={g} selected={g.id === selectedId} meta={meta(g)} playing={playing.has(g.id)} onSelect={onSelect} onPlay={onPlay} />
  );

  let content: ReactNode;
  if (segments) {
    const items: ReactNode[] = [];
    segments.forEach((s, n) => {
      if (s.kind === "space") {
        items.push(<div key={`vgap-${n}`} className="vgap" style={{ height: s.size }} aria-hidden />);
        return;
      }
      for (let r = s.first; r <= s.last; r++) {
        for (const g of games.slice(r * cols, r * cols + cols)) items.push(card(g));
        if (showStrip && r === selRow) items.push(renderStrip(games[index], notch));
      }
    });
    content = items;
  } else {
    content = games.map((g, i) => (
      <Fragment key={g.id}>
        {card(g)}
        {showStrip && i === rowEnd && renderStrip(games[index], notch)}
      </Fragment>
    ));
  }

  return (
    <div
      className="grid"
      ref={ref}
      data-cols={cols}
      style={{ ["--card-min" as string]: `${CARD_MIN}px`, ["--gap-x" as string]: `${GAP_X}px` }}
    >
      {content}
    </div>
  );
}
