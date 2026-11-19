import { useMemo } from "react";
import { MOOD_LABEL } from "../lib/api";
import { libraryMoods, libraryTags } from "../lib/library";
import type { SectionContext } from "../lib/sections";
import { EmptyState } from "./States";

/**
 * Section « Tags » : les ambiances puis tous les tags Steam de la bibliothèque (jeux non masqués),
 * du plus fréquent au moins fréquent. Un clic montre les jeux correspondants dans « Tous les jeux ».
 */
export function TagsView({ games, showTag, showMood }: SectionContext) {
  const tags = useMemo(() => libraryTags(games), [games]);
  const moods = useMemo(() => libraryMoods(games).filter((m) => m.count > 0), [games]);
  const visible = games.filter((g) => !g.hidden);
  const known = visible.filter((g) => g.details).length;

  if (tags.length === 0 && moods.length === 0) {
    return (
      <EmptyState title="Aucun tag pour l'instant">
        Les tags viennent de la boutique Steam, pour les jeux Steam et ceux que Ludex y a retrouvés. Ils arrivent quelques secondes après le scan, avec une connexion à Internet
      </EmptyState>
    );
  }
  return (
    <section aria-labelledby="tags-title">
      <div className="bar">
        <h2 id="tags-title">Tags<span>{tags.length}</span></h2>
      </div>
      {moods.length > 0 && (
        <>
          <h3 className="tags__head">Ambiances</h3>
          <ul className="tags">
            {moods.map(({ mood, count }) => (
              <li key={mood}>
                <button type="button" className="chip is-mood" data-nav onClick={() => showMood(mood)}>
                  {MOOD_LABEL[mood]} <em>{count}</em>
                </button>
              </li>
            ))}
          </ul>
        </>
      )}
      <h3 className="tags__head">Tags Steam</h3>
      <ul className="tags">
        {tags.map(({ name, count }) => (
          <li key={name}>
            <button type="button" className="chip" data-nav onClick={() => showTag(name)}>
              {name} <em>{count}</em>
            </button>
          </li>
        ))}
      </ul>
      <p className="tags__note">
        Votés par les joueurs sur la boutique Steam : {known} jeu{known > 1 ? "x" : ""} sur {visible.length} en {known > 1 ? "ont" : "a"}. Les jeux sans fiche Steam n'en ont pas.
      </p>
    </section>
  );
}
