import { memo } from "react";
import { PLATFORM_LABEL, type Game } from "../lib/api";
import { installBadge } from "../lib/format";
import { achievementPercent } from "../lib/library";
import { Cover } from "./Cover";
import { LauncherIcon } from "./LauncherIcon";
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
  /** Étoile au survol : ajoute aux favoris ou en retire. */
  onFavorite: (game: Game) => void;
}

export const GameCard = memo(function GameCard({ game, selected, meta, playing, onSelect, onPlay, onFavorite }: Props) {
  const badge = installBadge(game.installState);
  const percent = achievementPercent(game);
  return (
    <div className={`card ${selected ? "is-sel" : ""}`} data-game={game.id}>
      <div className="card__art">
        <button
          type="button"
          className="card__hit"
          aria-pressed={selected}
          aria-label={`${game.title}${game.favorite ? ", favori" : ""}${playing ? ", en jeu" : badge ? `, ${badge.toLowerCase()}` : ""}`}
          onClick={() => onSelect(game)}
          onDoubleClick={() => onPlay(game)}
        >
          <Cover game={game} progress />
          <span className="card__plat" title={PLATFORM_LABEL[game.platform]}><LauncherIcon platform={game.platform} size={13} /></span>
          {percent !== null && (
            <span className={`card__ach ${percent === 100 ? "is-perfect" : ""}`} title={`${game.achievements!.unlocked} / ${game.achievements!.total} succès`}>
              {percent} %
            </span>
          )}
        </button>
        {/* Hors du bouton de la jaquette (pas de bouton dans un bouton) ; toujours visible sur un favori. */}
        <button
          type="button"
          className={`card__star${game.favorite ? " is-on" : ""}`}
          tabIndex={-1}
          aria-pressed={game.favorite}
          aria-label={game.favorite ? `Retirer ${game.title} des favoris` : `Ajouter ${game.title} aux favoris`}
          title={game.favorite ? "Retirer des favoris (Ctrl+D)" : "Ajouter aux favoris (Ctrl+D)"}
          onClick={() => onFavorite(game)}
          onDoubleClick={(e) => e.stopPropagation()}
        >
          <StarIcon filled={game.favorite} badge />
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
