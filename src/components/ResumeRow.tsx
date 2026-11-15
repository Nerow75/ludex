import type { Game } from "../lib/api";
import { relativeTime } from "../lib/format";
import { Art, Logo } from "./Art";
import { PlayIcon } from "./Icons";

interface Props {
  games: Game[];
  /** Jeux dont une session est en cours. */
  playing: ReadonlySet<string>;
  onSelect: (game: Game) => void;
  onPlay: (game: Game) => void;
}

/** Les derniers jeux joués, en tuiles larges : le premier est mis en avant. */
export function ResumeRow({ games, playing, onSelect, onPlay }: Props) {
  if (games.length === 0) return null;
  return (
    <section className="resume" aria-labelledby="resume-title">
      <h2 id="resume-title">Reprendre</h2>
      <div className="resume__grid">
        {games.map((g, i) => {
          const live = playing.has(g.id);
          return (
            <div key={g.id} className={`tile ${i === 0 ? "tile--first" : ""}`}>
              <button
                type="button"
                className="tile__hit"
                onClick={() => onSelect(g)}
                onDoubleClick={() => onPlay(g)}
                aria-label={`${g.title}, ${live ? "en jeu" : relativeTime(g.lastPlayed)}`}
              >
                <Art game={g} />
                <Logo game={g} />
                <small>{live ? <span className="live">En jeu</span> : relativeTime(g.lastPlayed)}</small>
              </button>
              <button type="button" className="card__play" tabIndex={-1} aria-label={`Jouer à ${g.title}`} onClick={() => onPlay(g)}>
                <PlayIcon />
              </button>
            </div>
          );
        })}
      </div>
    </section>
  );
}
