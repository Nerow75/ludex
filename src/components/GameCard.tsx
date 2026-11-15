import { memo } from "react";
import { PLATFORM_LABEL, type Game } from "../lib/api";
import { installBadge } from "../lib/format";
import { Cover } from "./Cover";
import { PlayIcon, StarIcon } from "./Icons";

interface Props {
  game: Game;
  selected: boolean;
  /** Information contextuelle sous le titre (dernière session, taille…). */
  meta: string;
  /** Une session lancée depuis Ludex est en cours : remplace l'information sous le titre. */
  playing: boolean;
  onSelect: (game: Game) => void;
  onPlay: (game: Game) => void;
}

export const GameCard = memo(function GameCard({ game, selected, meta, playing, onSelect, onPlay }: Props) {
  const badge = installBadge(game.installState);
  return (
    <div className={`card ${selected ? "is-sel" : ""}`} data-game={game.id}>
      <div className="card__art">
        <button
          type="button"
          className="card__hit"
          aria-pressed={selected}
          aria-label={playing ? `${game.title}, en jeu` : badge ? `${game.title}, ${badge.toLowerCase()}` : game.title}
          onClick={() => onSelect(game)}
          onDoubleClick={() => onPlay(game)}
        >
          <Cover game={game} />
          {game.favorite && <span className="card__fav" aria-label="Favori"><StarIcon filled badge /></span>}
          {game.platform !== "steam" && <span className="card__plat">{PLATFORM_LABEL[game.platform]}</span>}
        </button>
        <button type="button" className="card__play" tabIndex={-1} aria-label={`Jouer à ${game.title}`} onClick={() => onPlay(game)}>
          <PlayIcon />
        </button>
      </div>
      <div className="card__text" onClick={() => onSelect(game)} onDoubleClick={() => onPlay(game)}>
        <span className="card__title">{game.title}</span>
        <span className="card__meta">
          {playing ? <span className="live">En jeu</span> : badge ? <span className="live is-pending">{badge}</span> : meta}
        </span>
      </div>
    </div>
  );
});
