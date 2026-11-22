import { useEffect, useMemo, useState } from "react";
import { showcase, type Showcase, type ShowcaseUnlock } from "../lib/api";
import { formatAchievements, formatDate, formatPercent, relativeTime } from "../lib/format";
import { t } from "../lib/i18n";
import type { SectionContext } from "../lib/sections";
import { AchievementIcon } from "./AchievementList";
import { Cover } from "./Cover";
import { EmptyState } from "./States";

type Load = { state: "loading" } | { state: "ready"; data: Showcase } | { state: "error" };

/** Infobulle d'un succès : description et date complète. */
function unlockTitle(u: ShowcaseUnlock): string | undefined {
  return [u.description, u.unlockedAt && t.extras.showcase.unlockedOn(formatDate(u.unlockedAt))].filter(Boolean).join("\n") || undefined;
}

/**
 * Section « Vitrine » : succès Steam débloqués, jeux à 100 %, podium des trois succès les plus rares
 * puis la suite du top 10, derniers déblocages. Un clic ouvre la fiche du jeu.
 */
export function ShowcaseView({ games, openGame }: SectionContext) {
  const byId = useMemo(() => new Map(games.map((g) => [g.id, g])), [games]);
  // Relu quand un rescan change le nombre de succès débloqués (après une partie, F5…).
  const unlockedSum = useMemo(() => games.reduce((n, g) => n + (g.achievements?.unlocked ?? 0), 0), [games]);
  const [load, setLoad] = useState<Load>({ state: "loading" });

  useEffect(() => {
    let alive = true;
    showcase()
      .then((data) => alive && setLoad({ state: "ready", data }))
      .catch(() => alive && setLoad({ state: "error" }));
    return () => {
      alive = false;
    };
  }, [unlockedSum]);

  const text = t.extras.showcase;
  const open = (id: string) => {
    const game = byId.get(id);
    if (game) openGame(game);
  };

  if (load.state === "loading") {
    return <p className="vitrine__note" aria-busy="true">{text.loading}</p>;
  }
  if (load.state === "error") {
    return <EmptyState title={text.errorTitle}>{text.errorText}</EmptyState>;
  }
  const s = load.data;
  if (s.games === 0) {
    return <EmptyState title={text.emptyTitle}>{text.emptyText}</EmptyState>;
  }
  const podium = s.rarest.slice(0, 3);
  const rest = s.rarest.slice(3);
  const rate = s.total > 0 ? Math.round((s.unlocked / s.total) * 100) : 0;

  return (
    <section className="vitrine" aria-labelledby="vitrine-title" data-nav-2d>
      <div className="bar">
        <h2 id="vitrine-title">{text.title}</h2>
      </div>
      <dl className="stats vitrine__stats">
        <div><dt>{text.unlocked}</dt><dd>{formatAchievements(s.unlocked, s.total)}</dd></div>
        <div><dt>{text.progress}</dt><dd>{text.percent(rate)}</dd></div>
        <div><dt>{text.perfect}</dt><dd>{s.perfect.length}</dd></div>
        <div><dt>{text.withAchievements}</dt><dd>{s.games}</dd></div>
      </dl>

      {/* Deux colonnes quand la place le permet : les plus rares, puis le reste. */}
      <div className="vitrine__cols">
        <div className="vitrine__col">
          <h3 className="vitrine__h">{text.rarest}</h3>
          {s.rarest.length === 0 ? (
            <p className="vitrine__note">
              {s.unlocked === 0 ? text.noneUnlocked : text.noPercent}
            </p>
          ) : (
            <>
              <ol className="podium">
                {podium.map((u, i) => (
                  <li key={`${u.gameId}/${u.apiName}`}>
                    <button type="button" className="podium__item" data-nav onClick={() => open(u.gameId)} title={unlockTitle(u)}>
                      <span className="podium__rank">{i + 1}</span>
                      <AchievementIcon src={u.icon} className="ach-icon--l" />
                      <span className="podium__name">{u.name}</span>
                      <span className="podium__game">{u.gameTitle}</span>
                      {u.percent !== null && <span className="podium__pct">{formatPercent(u.percent)}</span>}
                    </button>
                  </li>
                ))}
              </ol>
              {rest.length > 0 && (
                <ol className="vitrine__list" start={4}>
                  {rest.map((u, i) => (
                    <li key={`${u.gameId}/${u.apiName}`}>
                      <UnlockRow u={u} rank={i + 4} onOpen={open} side={u.percent !== null ? formatPercent(u.percent) : ""} />
                    </li>
                  ))}
                </ol>
              )}
            </>
          )}
        </div>
        <div className="vitrine__col">
          {s.perfect.length > 0 && (
            <>
              <h3 className="vitrine__h">{text.perfect}<span><span className="sr-only">, </span>{s.perfect.length}</span></h3>
              <ol className="vitrine__list">
                {s.perfect.map((p) => {
                  const game = byId.get(p.gameId);
                  return (
                    <li key={p.gameId}>
                      <button
                        type="button"
                        className="vitrine__row"
                        data-nav
                        onClick={() => open(p.gameId)}
                        title={p.completedAt ? text.completedOn(formatDate(p.completedAt)) : undefined}
                      >
                        {game ? <Cover game={game} className="vitrine__cover" /> : <AchievementIcon src={null} />}
                        <span className="vitrine__name">{p.gameTitle}</span>
                        <span className="vitrine__meta">{text.count(p.total)}</span>
                        <span className="vitrine__side">{p.completedAt ? relativeTime(p.completedAt) : ""}</span>
                      </button>
                    </li>
                  );
                })}
              </ol>
            </>
          )}

          {s.recent.length > 0 && (
            <>
              <h3 className="vitrine__h">{text.recent}</h3>
              <ol className="vitrine__list">
                {s.recent.map((u) => (
                  <li key={`${u.gameId}/${u.apiName}`}>
                    <UnlockRow u={u} onOpen={open} side={u.unlockedAt ? relativeTime(u.unlockedAt) : ""} />
                  </li>
                ))}
              </ol>
            </>
          )}
        </div>
      </div>
    </section>
  );
}

function UnlockRow({ u, rank, side, onOpen }: { u: ShowcaseUnlock; rank?: number; side: string; onOpen: (id: string) => void }) {
  return (
    <button type="button" className="vitrine__row" data-nav onClick={() => onOpen(u.gameId)} title={unlockTitle(u)}>
      {rank !== undefined && <span className="vitrine__rank">{rank}</span>}
      <AchievementIcon src={u.icon} />
      <span className="vitrine__name">{u.name}</span>
      <span className="vitrine__meta">{u.gameTitle}</span>
      <span className="vitrine__side">{side}</span>
    </button>
  );
}
