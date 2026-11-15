import { PLATFORM_LABEL, type Game } from "../lib/api";
import { formatPlaytime, formatSize, relativeTime } from "../lib/format";
import { Art, Logo } from "./Art";
import { CloseIcon, EyeOffIcon, FolderIcon, PlayIcon, StarIcon } from "./Icons";

interface Props {
  game: Game;
  /** Position horizontale (px) du centre de la jaquette, pour la flèche. */
  notch: number;
  /** Une session lancée depuis Ludex est en cours. */
  playing: boolean;
  onPlay: (game: Game) => void;
  onFavorite: (game: Game) => void;
  onHide: (game: Game) => void;
  onOpenFolder: (game: Game) => void;
  onClose: () => void;
}

/** D'où vient le temps de jeu affiché, en une phrase. */
function playtimeOrigin(game: Game): string {
  if (game.playtimeSource === "launcher") return `Compté par ${PLATFORM_LABEL[game.platform]}, toutes sessions confondues`;
  if (game.playtimeSource === "ludex") return "Mesuré par Ludex, sur les parties lancées depuis Ludex";
  return "Ludex mesurera les parties lancées depuis Ludex";
}

/** Fiche du jeu, dépliée sous la rangée de sa jaquette. */
export function GameStrip({ game, notch, playing, onPlay, onFavorite, onHide, onOpenFolder, onClose }: Props) {
  return (
    <section className="strip" aria-label={`Fiche de ${game.title}`} style={{ ["--notch" as string]: `${notch}px` }}>
      <div className="strip__art">
        <Art game={game} />
        <Logo game={game} />
      </div>
      <div className="strip__info">
        <div className="strip__eyebrow">
          <span className="eyebrow">{PLATFORM_LABEL[game.platform]}{game.isTool ? " · Application" : ""}</span>
          {playing && <span className="live">En jeu</span>}
        </div>
        <h2>{game.title}</h2>
        <dl className="stats">
          <div title={playtimeOrigin(game)}><dt>Temps de jeu</dt><dd>{formatPlaytime(game.playtimeMinutes)}</dd></div>
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
            title={game.favorite ? "Retirer des favoris (Ctrl+D)" : "Ajouter aux favoris (Ctrl+D)"}
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
