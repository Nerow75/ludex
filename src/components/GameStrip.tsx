import { useEffect, useState } from "react";
import { gameHistory, PLATFORM_LABEL, type Game, type PastSession } from "../lib/api";
import { formatDuration, formatPlaytime, formatSize, relativeTime } from "../lib/format";
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

/** Dernière session mesurée par Ludex, relue à l'ouverture de la fiche et à la fin d'une partie. */
function useLastSession(id: string, playing: boolean): PastSession | null {
  const [last, setLast] = useState<PastSession | null>(null);
  useEffect(() => {
    let alive = true;
    gameHistory(id)
      .then((sessions) => alive && setLast(sessions[0] ?? null))
      .catch(() => alive && setLast(null)); // sans historique, la date seule reste affichée
    return () => {
      alive = false;
    };
  }, [id, playing]);
  return last?.gameId === id ? last : null;
}

/** « il y a 2 jours, 1 h 20 » : la durée n'est ajoutée que si la session mesurée est la dernière connue. */
function lastSessionText(game: Game, last: PastSession | null): { text: string; title?: string } {
  const end = last ? last.start + last.secs : 0;
  const launcherDate = game.lastPlayed ?? 0;
  // Joué depuis hors de Ludex (date du launcher bien plus récente) : la durée mesurée serait trompeuse.
  // Marge de 10 min : un launcher peut dater la partie de sa fermeture, après le dernier relevé de Ludex.
  if (!last || end + 600 < launcherDate) return { text: relativeTime(game.lastPlayed) };
  const title = last.launcher
    ? `Session observée par Ludex. Le temps de jeu reste celui de ${PLATFORM_LABEL[game.platform]}.`
    : "Session mesurée par Ludex";
  return { text: `${relativeTime(Math.max(end, launcherDate))}, ${formatDuration(last.secs)}`, title };
}

/** Fiche du jeu, dépliée sous la rangée de sa jaquette. */
export function GameStrip({ game, notch, playing, onPlay, onFavorite, onHide, onOpenFolder, onClose }: Props) {
  const lastSession = lastSessionText(game, useLastSession(game.id, playing));
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
          <div title={lastSession.title}><dt>Dernière session</dt><dd>{lastSession.text}</dd></div>
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
