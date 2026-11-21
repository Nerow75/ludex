import { useEffect, useMemo, useRef } from "react";
import { PLATFORM_LABEL, STATUS_LABEL, STATUSES, type Game, type Status } from "../lib/api";
import { cleanTitle } from "../lib/format";
import type { SectionContext } from "../lib/sections";
import { Cover } from "./Cover";
import { EmptyState } from "./States";

/** Événement envoyé par la manette (LT/RT) à l'élément qui a le focus : `detail` = -1 ou 1. */
export const SHIFT_EVENT = "ludex-shift";

const byRecent = (a: Game, b: Game) =>
  (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0) || cleanTitle(a.title).localeCompare(cleanTitle(b.title), "fr", { sensitivity: "base" });

/** Statut voisin dans l'ordre des colonnes ; rien avant « À faire » ni après « Abandonné ». */
function neighbour(status: Status, delta: number): Status | null {
  const next = STATUSES[STATUSES.indexOf(status) + delta];
  return next ?? null;
}

/**
 * Section « Progression » : les jeux suivis, rangés en quatre colonnes par statut. Clavier :
 * Maj+← et Maj+→ passent le jeu au statut voisin, Suppr lui retire son statut. Manette : LT/RT.
 */
export function ProgressBoard({ games, playing, openGame, play, setStatus }: SectionContext) {
  const columns = useMemo(() => {
    const by = new Map<Status, Game[]>(STATUSES.map((s) => [s, []]));
    for (const g of games) if (g.status && !g.hidden) by.get(g.status)?.push(g);
    by.forEach((list) => list.sort(byRecent));
    return by;
  }, [games]);
  const total = [...columns.values()].reduce((n, list) => n + list.length, 0);

  // Le focus suit le jeu dans sa nouvelle colonne (l'ancien bouton disparaît au changement).
  const follow = useRef<string | null>(null);
  const root = useRef<HTMLElement>(null);
  useEffect(() => {
    const id = follow.current;
    if (!id) return;
    follow.current = null;
    root.current?.querySelector<HTMLElement>(`[data-progress="${CSS.escape(id)}"]`)?.focus();
  }, [games]);

  const move = (game: Game, delta: number) => {
    if (!game.status) return;
    const next = neighbour(game.status, delta);
    if (!next) return;
    follow.current = game.id;
    setStatus(game, next);
  };

  // LT/RT de la manette : relayés par l'application sous forme d'événement sur le jeu qui a le focus.
  const live = useRef({ games, move });
  live.current = { games, move };
  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const onShift = (e: Event) => {
      const id = (e.target as HTMLElement).closest<HTMLElement>("[data-progress]")?.dataset.progress;
      const game = live.current.games.find((g) => g.id === id);
      if (game) live.current.move(game, (e as CustomEvent<number>).detail);
    };
    el.addEventListener(SHIFT_EVENT, onShift);
    return () => el.removeEventListener(SHIFT_EVENT, onShift);
  }, [total > 0]);

  const onKeyDown = (game: Game) => (e: React.KeyboardEvent) => {
    if (e.shiftKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
      e.preventDefault();
      e.stopPropagation(); // pas de navigation globale (barre latérale)
      move(game, e.key === "ArrowLeft" ? -1 : 1);
    } else if (e.key === "Delete") {
      e.preventDefault();
      e.stopPropagation();
      setStatus(game, null);
    }
  };

  if (total === 0) {
    return (
      <EmptyState title="Aucun jeu suivi pour l'instant">
        Ouvre la fiche d'un jeu et choisis un statut : À faire, En cours, Terminé ou Abandonné. Il apparaîtra ici, rangé dans sa colonne
      </EmptyState>
    );
  }
  return (
    <section aria-labelledby="progress-title" ref={root}>
      <div className="bar">
        <h2 id="progress-title">Progression<span><span className="sr-only">, </span>{total}</span></h2>
        <p className="board__hint">
          <kbd>Maj</kbd><kbd>←</kbd><kbd>→</kbd> ou <kbd>LT</kbd><kbd>RT</kbd> change le statut, <kbd>Suppr</kbd> le retire
        </p>
      </div>
      <div className="board">
        {STATUSES.map((status) => {
          const list = columns.get(status) ?? [];
          return (
            <section key={status} className="board__col" aria-labelledby={`board-${status}`}>
              <h3 id={`board-${status}`}>{STATUS_LABEL[status]}<span><span className="sr-only">, </span>{list.length}</span></h3>
              {list.length === 0 ? (
                <p className="board__empty">Aucun jeu</p>
              ) : (
                <ul>
                  {list.map((g) => (
                    <li key={g.id}>
                      <button
                        type="button"
                        className="board__item"
                        data-nav
                        data-progress={g.id}
                        onClick={() => openGame(g)}
                        onDoubleClick={() => play(g)}
                        onKeyDown={onKeyDown(g)}
                        aria-label={`${g.title}, ${STATUS_LABEL[status]}. Maj+flèches pour changer de statut`}
                        title={g.note ?? undefined}
                      >
                        <Cover game={g} className="board__cover" />
                        <span className="board__text">
                          <b>{g.title}</b>
                          <small>{playing.has(g.id) ? <span className="live">En jeu</span> : g.note ?? PLATFORM_LABEL[g.platform]}</small>
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </section>
          );
        })}
      </div>
    </section>
  );
}
