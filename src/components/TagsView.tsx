import { useMemo } from "react";
import { MOOD_LABEL, type Game, type Mood } from "../lib/api";
import { genreLabel } from "../lib/format";
import { t } from "../lib/i18n";
import { libraryGenres, libraryMoods } from "../lib/library";
import type { SectionContext } from "../lib/sections";
import { Cover } from "./Cover";
import { EmptyState } from "./States";

/** Trois jaquettes pour illustrer une carte : les jeux joués le plus récemment d'abord. */
function sample(games: Game[]): Game[] {
  return [...games].sort((a, b) => (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0) || a.title.localeCompare(b.title)).slice(0, 3);
}

function ExploreCard({ title, hint, count, games, mood, onOpen }: { title: string; hint?: string; count: number; games: Game[]; mood?: boolean; onOpen: () => void }) {
  return (
    <li>
      <button type="button" className={`explore__card${mood ? " is-mood" : ""}`} data-nav onClick={onOpen} aria-label={t.library.explore.card(title, count)}>
        <span className="explore__covers" aria-hidden>
          {sample(games).map((g) => <Cover key={g.id} game={g} className="explore__cover" />)}
        </span>
        <span className="explore__text">
          <b>{title}</b>
          {hint && <small>{hint}</small>}
        </span>
        <em>{count}</em>
      </button>
    </li>
  );
}

/**
 * Section « Explorer » : les ambiances puis les genres de la bibliothèque (jeux non masqués), en
 * cartes. Un clic montre les jeux correspondants dans « Tous les jeux ». Les tags Steam, trop
 * nombreux pour une page, restent dans la liste « Tag » de la bibliothèque.
 */
export function TagsView({ games, showMood, showGenre }: SectionContext) {
  const visible = useMemo(() => games.filter((g) => !g.hidden && !g.isTool), [games]);
  const moods = useMemo(() => libraryMoods(visible).filter((m) => m.count > 0), [visible]);
  const genres = useMemo(() => libraryGenres(visible), [visible]);
  const known = visible.filter((g) => g.details).length;
  const explore = t.library.explore;

  if (genres.length === 0 && moods.length === 0) {
    return (
      <EmptyState title={explore.emptyTitle}>{explore.emptyBody}</EmptyState>
    );
  }
  const withMood = (mood: Mood) => visible.filter((g) => g.details?.moods.includes(mood));
  const withGenre = (genre: string) => visible.filter((g) => g.details?.genres.includes(genre));
  return (
    <section aria-labelledby="explore-title" className="explore" data-nav-2d>
      <div className="bar">
        <h2 id="explore-title">{explore.title}</h2>
      </div>
      {moods.length > 0 && (
        <>
          <h3 className="explore__h">{explore.byMood}</h3>
          <ul className="explore__grid">
            {moods.map(({ mood, count }) => (
              <ExploreCard key={mood} title={MOOD_LABEL[mood]} hint={explore.hint[mood]} count={count} games={withMood(mood)} mood onOpen={() => showMood(mood)} />
            ))}
          </ul>
        </>
      )}
      {genres.length > 0 && (
        <>
          <h3 className="explore__h">{explore.byGenre}</h3>
          <ul className="explore__grid">
            {genres.map(({ name, count }) => (
              <ExploreCard key={name} title={genreLabel(name)} count={count} games={withGenre(name)} onOpen={() => showGenre(name)} />
            ))}
          </ul>
        </>
      )}
      <p className="tags__note">{explore.note(known, visible.length)}</p>
    </section>
  );
}
