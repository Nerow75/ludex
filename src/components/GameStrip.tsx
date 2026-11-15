import { PLATFORM_LABEL, type Game } from "../lib/api";
import { formatSize, relativeTime } from "../lib/format";
import { Art, Logo } from "./Art";
import { CloseIcon, EyeOffIcon, FolderIcon, PlayIcon, StarIcon } from "./Icons";

interface Props {
  game: Game;
  /** Position horizontale (px) du centre de la jaquette, pour la flèche. */
  notch: number;
  onPlay: (game: Game) => void;
  onFavorite: (game: Game) => void;
  onHide: (game: Game) => void;
  onOpenFolder: (game: Game) => void;
  onClose: () => void;
}

/** Fiche du jeu, dépliée sous la rangée de sa jaquette. */
export function GameStrip({ game, notch, onPlay, onFavorite, onHide, onOpenFolder, onClose }: Props) {
  return (
    <section className="strip" aria-label={`Fiche de ${game.title}`} style={{ ["--notch" as string]: `${notch}px` }}>
      <div className="strip__art">
        <Art game={game} />
        <Logo game={game} />
      </div>
      <div className="strip__info">
        <span className="eyebrow">{PLATFORM_LABEL[game.platform]}{game.isTool ? " · Application" : ""}</span>
        <h2>{game.title}</h2>
        <dl className="stats">
          <div><dt>Dernière session</dt><dd>{relativeTime(game.lastPlayed)}</dd></div>
          <div><dt>Taille</dt><dd>{formatSize(game.sizeOnDisk)}</dd></div>
          <div><dt>Launcher</dt><dd>{PLATFORM_LABEL[game.platform]}</dd></div>
        </dl>
        <div className="strip__acts">
          <button type="button" className="btn-play" onClick={() => onPlay(game)}><PlayIcon /> Jouer</button>
          <button
            type="button"
            className="btn-icon"
            aria-pressed={game.favorite}
            aria-label={game.favorite ? "Retirer des favoris" : "Ajouter aux favoris"}
            title={game.favorite ? "Retirer des favoris (F)" : "Ajouter aux favoris (F)"}
            onClick={() => onFavorite(game)}
          >
            <StarIcon filled={game.favorite} />
          </button>
          {game.installDir && (
            <button type="button" className="btn-icon" aria-label="Ouvrir le dossier" title="Ouvrir le dossier" onClick={() => onOpenFolder(game)}>
              <FolderIcon />
            </button>
          )}
          <button
            type="button"
            className="btn-icon"
            aria-label={game.hidden ? "Ne plus masquer" : "Masquer de la bibliothèque"}
            title={game.hidden ? "Ne plus masquer" : "Masquer de la bibliothèque"}
            onClick={() => onHide(game)}
          >
            <EyeOffIcon />
          </button>
        </div>
        {game.installDir && <p className="path" title={game.installDir}>{game.installDir}</p>}
      </div>
      <button type="button" className="strip__close" aria-label="Fermer la fiche" title="Fermer (Échap)" onClick={onClose}>
        <CloseIcon />
      </button>
    </section>
  );
}
