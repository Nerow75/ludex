import { useEffect, useMemo, useState } from "react";
import { lastSessions, PLATFORM_LABEL, type PastSession } from "../lib/api";
import { formatDate } from "../lib/format";
import { recentGames } from "../lib/library";
import type { SectionContext } from "../lib/sections";
import { Cover } from "./Cover";
import { lastSessionText } from "./GameStrip";
import { PlayIcon } from "./Icons";
import { EmptyState } from "./States";

/**
 * Section « Historique » : les jeux lancés, du plus récent au plus ancien (date du launcher ou de
 * Ludex), avec la durée de la dernière session quand Ludex l'a mesurée et qu'elle est la dernière connue.
 */
export function HistoryList({ games, playing, openGame, play }: SectionContext) {
  const list = useMemo(() => recentGames(games, games.length), [games]);
  const [sessions, setSessions] = useState<ReadonlyMap<string, PastSession>>(() => new Map());

  // Relu à l'ouverture et quand une partie commence ou se termine.
  useEffect(() => {
    let alive = true;
    lastSessions()
      .then((all) => alive && setSessions(new Map(all.map((s) => [s.gameId, s]))))
      .catch(() => {}); // sans historique, les dates seules restent affichées
    return () => {
      alive = false;
    };
  }, [playing]);

  if (list.length === 0) {
    return <EmptyState title="Aucun jeu lancé pour l'instant">Les jeux lancés depuis Ludex ou leur launcher apparaîtront ici, le plus récent en haut</EmptyState>;
  }
  return (
    <section aria-labelledby="history-title">
      <div className="bar">
        <h2 id="history-title">Historique<span>{list.length}</span></h2>
      </div>
      <ol className="hist">
        {list.map((g) => {
          const live = playing.has(g.id);
          const last = lastSessionText(g, sessions.get(g.id) ?? null);
          return (
            <li key={g.id} className="hist__item">
              <button
                type="button"
                className="hist__row"
                data-nav
                onClick={() => openGame(g)}
                onDoubleClick={() => play(g)}
                title={g.lastPlayed ? `${formatDate(g.lastPlayed)}${last.title ? `. ${last.title}` : ""}` : undefined}
              >
                <Cover game={g} className="hist__cover" />
                <span className="hist__title">{g.title}</span>
                <span className="hist__meta">{PLATFORM_LABEL[g.platform]}</span>
                <span className="hist__when">{live ? <span className="live">En jeu</span> : last.text}</span>
              </button>
              <button type="button" className="card__play" tabIndex={-1} aria-label={`Jouer à ${g.title}`} onClick={() => play(g)}>
                <PlayIcon />
              </button>
            </li>
          );
        })}
      </ol>
    </section>
  );
}
