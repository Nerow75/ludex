import { useState } from "react";
import { PLATFORM_LABEL, type Game } from "../lib/api";
import { Cover } from "./Cover";
import { FolderIcon, PlayIcon } from "./Icons";

interface Props {
  game: Game;
  onPlay: () => void;
  onOpenFolder: () => void;
}

export function Hero({ game, onPlay, onOpenFolder }: Props) {
  const [heroFailed, setHeroFailed] = useState<string | null>(null);
  const showHero = game.heroUrl && heroFailed !== game.id;

  return (
    <section className="hero" aria-label={`Jeu sélectionné : ${game.title}`}>
      <span className="hero__glow" aria-hidden />
      {showHero && (
        <img
          key={game.id}
          className="hero__art"
          src={game.heroUrl!}
          alt=""
          draggable={false}
          onError={() => setHeroFailed(game.id)}
        />
      )}
      <div className="hero__info">
        <span className="platform">Installé · via {PLATFORM_LABEL[game.platform]}</span>
        <h1 className="hero__title">{game.title}</h1>
        {game.installDir && <p className="hero__path" title={game.installDir}>{game.installDir}</p>}
        <div className="hero__actions">
          <button className="btn-play" type="button" onClick={onPlay}>
            <PlayIcon /> Jouer
          </button>
          {game.installDir && (
            <button className="btn-ghost" type="button" onClick={onOpenFolder}>
              <FolderIcon /> Ouvrir le dossier
            </button>
          )}
        </div>
      </div>
      <Cover key={game.id} game={game} className="hero__cover" />
    </section>
  );
}
