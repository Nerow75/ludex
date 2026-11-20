import { useMemo } from "react";
import { MOOD_LABEL, type Game, type Mood } from "../lib/api";
import { libraryGenres, libraryMoods } from "../lib/library";
import type { SectionContext } from "../lib/sections";
import { Cover } from "./Cover";
import { EmptyState } from "./States";

const MOOD_HINT: Record<Mood, string> = {
  chill: "Posé, sans pression",
  intense: "Action et tension",
  story: "Récit et personnages",
  friends: "Coop et écran partagé",
};

/** Trois jaquettes pour illustrer une carte : les jeux joués le plus récemment d'abord. */
function sample(games: Game[]): Game[] {
  return [...games].sort((a, b) => (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0) || a.title.localeCompare(b.title)).slice(0, 3);
}

function ExploreCard({ title, hint, count, games, mood, onOpen }: { title: string; hint?: string; count: number; games: Game[]; mood?: boolean; onOpen: () => void }) {
  return (
    <li>
      <button type="button" className={`explore__card${mood ? " is-mood" : ""}`} data-nav onClick={onOpen} aria-label={`${title}, ${count} jeu${count > 1 ? "x" : ""}`}>
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

  if (genres.length === 0 && moods.length === 0) {
    return (
      <EmptyState title="Rien à explorer pour l'instant">
        Ambiances et genres viennent de la boutique Steam, pour les jeux Steam et ceux que Ludex y a retrouvés. Ils arrivent quelques secondes après le scan, avec une connexion à Internet
      </EmptyState>
    );
  }
  const withMood = (mood: Mood) => visible.filter((g) => g.details?.moods.includes(mood));
  const withGenre = (genre: string) => visible.filter((g) => g.details?.genres.includes(genre));
  return (
    <section aria-labelledby="explore-title" className="explore" data-nav-2d>
      <div className="bar">
        <h2 id="explore-title">Explorer</h2>
      </div>
      {moods.length > 0 && (
        <>
          <h3 className="explore__h">Selon l'envie</h3>
          <ul className="explore__grid">
            {moods.map(({ mood, count }) => (
              <ExploreCard key={mood} title={MOOD_LABEL[mood]} hint={MOOD_HINT[mood]} count={count} games={withMood(mood)} mood onOpen={() => showMood(mood)} />
            ))}
          </ul>
        </>
      )}
      {genres.length > 0 && (
        <>
          <h3 className="explore__h">Par genre</h3>
          <ul className="explore__grid">
            {genres.map(({ name, count }) => (
              <ExploreCard key={name} title={name} count={count} games={withGenre(name)} onOpen={() => showGenre(name)} />
            ))}
          </ul>
        </>
      )}
      <p className="tags__note">
        Tirés de la boutique Steam : {known} jeu{known > 1 ? "x" : ""} sur {visible.length} {known > 1 ? "ont" : "a"} une fiche. Les tags détaillés sont dans la liste « Tag » de « Tous les jeux ».
      </p>
    </section>
  );
}
