import type { Game } from "../lib/api";
import { relativeTime } from "../lib/format";
import { Art, Logo } from "./Art";
import { PlayIcon } from "./Icons";

interface Props {
  games: Game[];
  onSelect: (game: Game) => void;
  onPlay: (game: Game) => void;
}

/** Les derniers jeux joués, en tuiles larges : le premier est mis en avant. */
export function ResumeRow({ games, onSelect, onPlay }: Props) {
  if (games.length === 0) return null;
  return (
    <section className="resume" aria-labelledby="resume-title">
      <h2 id="resume-title">Reprendre</h2>
      <div className="resume__grid">
        {games.map((g, i) => (
          <div key={g.id} className={`tile ${i === 0 ? "tile--first" : ""}`}>
            <button type="button" className="tile__hit" onClick={() => onSelect(g)} onDoubleClick={() => onPlay(g)} aria-label={`${g.title}, ${relativeTime(g.lastPlayed)}`}>
              <Art game={g} />
              <Logo game={g} />
              <small>{relativeTime(g.lastPlayed)}</small>
            </button>
            <button type="button" className="card__play" tabIndex={-1} aria-label={`Jouer à ${g.title}`} onClick={() => onPlay(g)}>
              <PlayIcon />
            </button>
          </div>
        ))}
      </div>
    </section>
  );
}
