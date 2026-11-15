import { memo } from "react";
import { PLATFORM_LABEL, type Game } from "../lib/api";
import { Cover } from "./Cover";
import { PlayIcon, StarIcon } from "./Icons";

interface Props {
  game: Game;
  selected: boolean;
  /** Information contextuelle sous le titre (dernière session, taille…). */
  meta: string;
  onSelect: (game: Game) => void;
  onPlay: (game: Game) => void;
}

export const GameCard = memo(function GameCard({ game, selected, meta, onSelect, onPlay }: Props) {
  return (
    <div className={`card ${selected ? "is-sel" : ""}`} data-game={game.id}>
      <div className="card__art">
        <button
          type="button"
          className="card__hit"
          aria-pressed={selected}
          aria-label={game.title}
          onClick={() => onSelect(game)}
          onDoubleClick={() => onPlay(game)}
        >
          <Cover game={game} />
          {game.favorite && <span className="card__fav" aria-label="Favori"><StarIcon filled size={13} /></span>}
          {game.platform !== "steam" && <span className="card__plat">{PLATFORM_LABEL[game.platform]}</span>}
        </button>
        <button type="button" className="card__play" tabIndex={-1} aria-label={`Jouer à ${game.title}`} onClick={() => onPlay(game)}>
          <PlayIcon />
        </button>
      </div>
      <div className="card__text" onClick={() => onSelect(game)} onDoubleClick={() => onPlay(game)}>
        <span className="card__title">{game.title}</span>
        <span className="card__meta">{meta}</span>
      </div>
    </div>
  );
});
