import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { journal, type Achievement, type JournalDay, type JournalGame, type JournalSession } from "../lib/api";
import { formatClock, formatDay, formatDuration, formatPercent, relativeDay } from "../lib/format";
import { t } from "../lib/i18n";
import type { SectionContext } from "../lib/sections";
import { AchievementIcon } from "./AchievementList";
import { Cover } from "./Cover";
import { CloseIcon } from "./Icons";
import { ScreenshotViewer, ShotTile } from "./ScreenshotViewer";
import { EmptyState } from "./States";

/** Vignettes de captures montrées par jeu et par jour ; la dernière porte le reste (« +4 »). */
const SHOTS_SHOWN = 6;

type Load = { state: "loading" } | { state: "error"; message: string } | { state: "ready"; days: JournalDay[]; next: string | null };

/** « 14:05, 1 h 20 », « Depuis la veille, 40 min », « Plusieurs sessions, 3 h » (entrée regroupée). */
function sessionText(s: JournalSession): string {
  const when = s.start === null ? t.activity.journal.severalSessions : s.continued ? t.activity.journal.sinceYesterday : formatClock(s.start);
  return `${when}, ${formatDuration(s.secs)}`;
}

/** Infobulle d'un succès : nom, description et part des joueurs. */
function achievementTitle(a: Achievement): string {
  const percent = a.globalPercent !== null ? t.activity.journal.ofPlayers(formatPercent(a.globalPercent)) : null;
  const when = a.unlockedAt ? t.activity.journal.unlockedAt(formatClock(a.unlockedAt)) : null;
  return [a.name, a.description, [when, percent].filter(Boolean).join(" · ")].filter(Boolean).join("\n");
}

/** Résumé d'un jeu dans un jour : « 2 sessions · 3 succès · 1 capture » (aussi dans l'onglet Journal de la fiche). */
export function gameSummary(g: JournalGame): string {
  const tj = t.activity.journal;
  return [
    g.sessions.length > 0 && tj.sessions(g.sessions.length),
    g.unlocked > 0 && tj.achievements(g.unlocked),
    g.shots.length > 0 && tj.shots(g.shots.length),
  ]
    .filter(Boolean)
    .join(" · ");
}

interface Viewing {
  day: number;
  game: number;
  index: number;
}

/**
 * Section « Journal » : la vie de joueur jour par jour, du plus récent au plus ancien. Par jour :
 * temps joué, jeux avec leurs sessions, succès débloqués (icônes) et captures (visionneuse). Les
 * jours plus anciens arrivent par pages quand on atteint le bas de la liste. Avec `journalGame`,
 * seulement l'histoire de ce jeu.
 */
export function JournalView({ games, playing, openGame, journalGame, showWholeJournal }: SectionContext) {
  const byId = useMemo(() => new Map(games.map((g) => [g.id, g])), [games]);
  // Relu quand un rescan change le nombre de succès débloqués, ou quand une partie commence ou finit.
  const unlockedSum = useMemo(() => games.reduce((n, g) => n + (g.achievements?.unlocked ?? 0), 0), [games]);
  const gameId = journalGame?.id ?? null;
  const [load, setLoad] = useState<Load>({ state: "loading" });
  const [more, setMore] = useState<"idle" | "loading" | "error">("idle");
  const [viewing, setViewing] = useState<Viewing | null>(null);
  // Numéro de la lecture en cours : une réponse d'une lecture dépassée est ignorée.
  const generation = useRef(0);
  const sentinel = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const gen = ++generation.current;
    setMore("idle");
    setViewing(null);
    journal(null, gameId)
      .then((page) => gen === generation.current && setLoad({ state: "ready", days: page.days, next: page.next }))
      .catch((e) => gen === generation.current && setLoad({ state: "error", message: String(e) }));
  }, [gameId, unlockedSum, playing]);

  const next = load.state === "ready" ? load.next : null;
  const loadMore = useCallback(() => {
    if (!next || more === "loading") return;
    const gen = generation.current;
    setMore("loading");
    journal(next, gameId)
      .then((page) => {
        if (gen !== generation.current) return;
        setLoad((l) => (l.state === "ready" ? { state: "ready", days: [...l.days, ...page.days], next: page.next } : l));
        setMore("idle");
      })
      .catch(() => gen === generation.current && setMore("error"));
  }, [next, more, gameId]);

  // Page suivante dès que le bas de la liste approche.
  useEffect(() => {
    const el = sentinel.current;
    if (!el || !next || more !== "idle") return;
    const observer = new IntersectionObserver((entries) => entries.some((e) => e.isIntersecting) && loadMore(), { rootMargin: "400px 0px" });
    observer.observe(el);
    return () => observer.disconnect();
  }, [next, more, loadMore]);

  const tj = t.activity.journal;
  const heading = (
    <div className="bar">
      <h2 id="journal-title">{journalGame ? tj.titleOf(journalGame.title) : tj.title}</h2>
      {journalGame && (
        <button type="button" className="btn-ghost journal__all" data-nav onClick={showWholeJournal}>
          <CloseIcon /> {tj.all}
        </button>
      )}
    </div>
  );

  if (load.state === "loading") {
    return <section aria-labelledby="journal-title">{heading}<p className="journal__note" aria-busy="true">{tj.loading}</p></section>;
  }
  if (load.state === "error") {
    return (
      <section aria-labelledby="journal-title">
        {heading}
        <EmptyState title={tj.unavailable}>{load.message}</EmptyState>
      </section>
    );
  }
  if (load.days.length === 0) {
    return (
      <section aria-labelledby="journal-title">
        {heading}
        <EmptyState title={tj.emptyTitle}>{tj.emptyBody}</EmptyState>
      </section>
    );
  }

  const viewed = viewing && load.days[viewing.day]?.games[viewing.game];

  return (
    <section className="journal" aria-labelledby="journal-title" data-nav-2d>
      {heading}
      <ol className="journal__days">
        {load.days.map((day, di) => (
          <li key={day.date} className="jday">
            <header className="jday__head">
              <h3>{relativeDay(day.date)}</h3>
              <span className="jday__date">{formatDay(day.date)}</span>
              {day.secs > 0 && <span className="jday__total" title={tj.dayTotal}>{formatDuration(day.secs)}</span>}
            </header>
            <ul className="jday__games">
              {day.games.map((g, gi) => {
                const game = g.gameId ? byId.get(g.gameId) : undefined;
                const rest = g.shots.length - SHOTS_SHOWN;
                const shown = rest > 0 ? g.shots.slice(0, SHOTS_SHOWN) : g.shots;
                const head = (
                  <>
                    {game ? <Cover game={game} className="jgame__cover" /> : <span className="jgame__cover jgame__cover--none" />}
                    <span className="jgame__title">{g.title}</span>
                    <span className="jgame__meta">{gameSummary(g)}</span>
                    {g.secs > 0 && <span className="jgame__time">{formatDuration(g.secs)}</span>}
                  </>
                );
                return (
                  <li key={g.gameId ?? "autres"} className="jgame">
                    {game ? (
                      <button type="button" className="jgame__head" data-nav onClick={() => openGame(game)} title={tj.openGame(g.title)}>
                        {head}
                      </button>
                    ) : (
                      <div className="jgame__head">{head}</div>
                    )}
                    {g.sessions.length > 0 && (
                      <p className="jgame__sessions">{g.sessions.map(sessionText).join(" · ")}</p>
                    )}
                    {g.achievements.length > 0 && (
                      <ul className="jgame__achs" aria-label={tj.unlocked(g.unlocked)}>
                        {g.achievements.map((a) => (
                          <li key={a.apiName} title={achievementTitle(a)}>
                            <AchievementIcon src={a.icon} />
                          </li>
                        ))}
                        {g.unlocked > g.achievements.length && <li className="jgame__achs-more">+{g.unlocked - g.achievements.length}</li>}
                      </ul>
                    )}
                    {shown.length > 0 && (
                      <div className="shots-row jgame__shots">
                        {shown.map((s, i) => (
                          <ShotTile
                            key={s.key}
                            shot={s}
                            nav
                            label={tj.shot(i + 1, g.title)}
                            more={i === shown.length - 1 && rest > 0 ? rest : undefined}
                            onOpen={() => setViewing({ day: di, game: gi, index: i })}
                          />
                        ))}
                      </div>
                    )}
                  </li>
                );
              })}
            </ul>
          </li>
        ))}
      </ol>
      <div ref={sentinel} className="journal__end">
        {next ? (
          <button type="button" className="btn-ghost" data-nav disabled={more === "loading"} onClick={loadMore}>
            {more === "loading" ? tj.loadingMore : more === "error" ? tj.retry : tj.more}
          </button>
        ) : (
          <p className="journal__note">{tj.start}</p>
        )}
      </div>
      {viewed && viewing && (
        <ScreenshotViewer
          shots={viewed.shots}
          index={viewing.index}
          title={viewed.title}
          onIndex={(index) => setViewing({ ...viewing, index })}
          onClose={() => setViewing(null)}
        />
      )}
    </section>
  );
}
