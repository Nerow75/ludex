import { Fragment, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import type { Game } from "../lib/api";
import { GameCard } from "./GameCard";

const CARD_MIN = 140;
const GAP_X = 14;

/** Nombre de colonnes réel de la grille : même calcul que `repeat(auto-fill, minmax(140px, 1fr))`. */
function useColumns() {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
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
  onSelect: (game: Game) => void;
  onPlay: (game: Game) => void;
  /** Fiche à insérer sous la rangée du jeu sélectionné ; reçoit la position de la flèche. */
  renderStrip: (game: Game, notch: number) => ReactNode;
}

export function LibraryGrid({ games, selectedId, open, meta, onSelect, onPlay, renderStrip }: Props) {
  const { ref, width, cols } = useColumns();
  const index = games.findIndex((g) => g.id === selectedId);
  const rowEnd = index < 0 ? -1 : Math.min(games.length - 1, Math.floor(index / cols) * cols + cols - 1);
  const cardWidth = (width - (cols - 1) * GAP_X) / cols;
  const notch = (index % cols) * (cardWidth + GAP_X) + cardWidth / 2;

  return (
    <div className="grid" ref={ref} style={{ ["--card-min" as string]: `${CARD_MIN}px`, ["--gap-x" as string]: `${GAP_X}px` }}>
      {games.map((g, i) => (
        <Fragment key={g.id}>
          <GameCard game={g} selected={g.id === selectedId} meta={meta(g)} onSelect={onSelect} onPlay={onPlay} />
          {open && i === rowEnd && width > 0 && renderStrip(games[index], notch)}
        </Fragment>
      ))}
    </div>
  );
}
