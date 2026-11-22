import { useEffect, useMemo, useState, type ReactNode } from "react";
import { loadStats, type StatsInfo } from "../lib/api";
import { formatDate, formatDuration, formatPlaytime } from "../lib/format";
import { nf, t, useLang } from "../lib/i18n";
import type { SectionContext } from "../lib/sections";
import { genreStats, launcherShares, monthBars, peak, playtimeTotals, sourceLabel, statsEmpty, topGames, type MonthBar, type PlaytimeTotals } from "../lib/stats";
import { parseDay } from "../lib/story";
import { Cover } from "./Cover";
import { LauncherIcon } from "./LauncherIcon";
import { EmptyState } from "./States";

/** « 68 jeux, temps des launchers », « 60 par le launcher, 8 par Ludex ». */
function totalsNote(totals: PlaytimeTotals): string {
  const a = t.activity;
  if (totals.played === 0) return a.stats.noGameTracked;
  if (totals.fromLudex === 0) return a.stats.launcherTimes(a.games(totals.played));
  if (totals.fromLauncher === 0) return a.stats.ludexTimes(a.games(totals.played));
  return a.stats.bothTimes(totals.fromLauncher, totals.fromLudex);
}
const dayText = (day: string) => formatDate(parseDay(day).getTime() / 1000);

function Tile({ label, value, note }: { label: string; value: string; note?: string | null }) {
  return (
    <div className="recap__stat">
      <span>{label}</span>
      <b>{value}</b>
      {note && <small title={note}>{note}</small>}
    </div>
  );
}

/** Histogramme mensuel en CSS (mêmes barres que le Récap), résumé pour les lecteurs d'écran. */
function MonthChart({ bars, label, format }: { bars: MonthBar[]; label: string; format: (n: number) => string }) {
  const max = Math.max(1, ...bars.map((b) => b.value));
  const top = peak(bars);
  const summary = top ? t.activity.stats.chartPeak(label, top.title, format(top.value)) : t.activity.stats.chartEmpty(label);
  return (
    <div className="recap__chart statpage__chart" role="img" aria-label={summary}>
      {bars.map((b, i) => (
        <div key={i} className={`recap__col${b.current ? " is-current" : ""}`} title={t.activity.barTitle(b.title, b.value > 0 ? format(b.value) : null)}>
          <div className="recap__track">
            <div className="recap__bar" style={{ height: `${b.value > 0 ? Math.max(2, (b.value / max) * 100) : 0}%` }} />
          </div>
          <span>{b.label}</span>
        </div>
      ))}
    </div>
  );
}

/** Une carte de la page : titre, provenance des chiffres, contenu. */
function Card({ id, area, title, source, children }: { id: string; area: string; title: string; source: string; children: ReactNode }) {
  return (
    <section className={`statpage__card statpage__card--${area}`} aria-labelledby={id}>
      <h3 id={id}>{title}</h3>
      <p className="statpage__source">{source}</p>
      {children}
    </section>
  );
}

/**
 * Section « Statistiques » : deux sources gardées à part. Le temps de chaque jeu (compteur du
 * launcher, sinon cumul de Ludex) sert au top, aux launchers et aux genres ; les sessions mesurées
 * par Ludex (commande `stats`) servent aux mois et aux séries ; les succès viennent du cache de Steam.
 */
export function StatsView({ games, playing, openGame, showGenre }: SectionContext) {
  // Libellés des genres et des launchers relus au changement de langue.
  const language = useLang();
  const [info, setInfo] = useState<StatsInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Relu quand une partie commence ou finit, et quand de nouveaux succès sont lus.
  const unlocked = useMemo(() => games.reduce((n, g) => n + (g.achievements?.unlocked ?? 0), 0), [games]);
  useEffect(() => {
    let alive = true;
    loadStats()
      .then((s) => {
        if (!alive) return;
        setInfo(s);
        setError(null);
      })
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [playing, unlocked]);

  const totals = useMemo(() => playtimeTotals(games), [games]);
  const top = useMemo(() => topGames(games), [games, language]);
  const launchers = useMemo(() => launcherShares(games), [games, language]);
  const genres = useMemo(() => genreStats(games), [games, language]);
  const a = t.activity;
  const ts = a.stats;

  const header = (
    <div className="bar">
      <h2 id="stats-title">{ts.title}</h2>
    </div>
  );
  if (error) {
    return (
      <section aria-labelledby="stats-title">
        {header}
        <EmptyState title={ts.unavailable}>{error}</EmptyState>
      </section>
    );
  }
  if (!info) return <section aria-labelledby="stats-title">{header}</section>;
  if (statsEmpty(games, info)) {
    return (
      <section aria-labelledby="stats-title">
        {header}
        <EmptyState title={ts.emptyTitle}>{ts.emptyBody}</EmptyState>
      </section>
    );
  }

  const hours = monthBars(info.months, (m) => m.secs);
  const unlocks = monthBars(info.months, (m) => m.achievements);
  const yearSecs = info.months.reduce((n, m) => n + m.secs, 0);
  const yearSessions = info.months.reduce((n, m) => n + m.sessions, 0);
  const yearDays = info.months.reduce((n, m) => n + m.days, 0);
  const yearUnlocks = info.months.reduce((n, m) => n + m.achievements, 0);
  const maxTop = Math.max(1, ...top.map((g) => g.playtimeMinutes ?? 0));
  const LUDEX_SOURCE = ts.ludexSource(info.historySince ? formatDate(info.historySince) : null);
  const LIBRARY_SOURCE = ts.librarySource;
  const percent = nf({ style: "percent", maximumFractionDigits: 0 });

  return (
    <section aria-labelledby="stats-title" className="statpage">
      {header}

      <div className="recap__stats statpage__tiles">
        <Tile
          label={ts.knownTime}
          value={totals.minutes > 0 ? formatPlaytime(totals.minutes) : ts.none}
          note={totalsNote(totals)}
        />
        <Tile
          label={ts.withLudex}
          value={yearSecs > 0 ? formatDuration(yearSecs) : a.zero}
          note={ts.sessionsDays(a.sessions(yearSessions), a.playedDays(yearDays))}
        />
        <Tile
          label={ts.bestStreak}
          value={a.days(info.bestStreak)}
          note={info.bestStreakEnd ? ts.streakUntil(dayText(info.bestStreakEnd)) : ts.noLudexGame}
        />
        <Tile
          label={ts.currentStreak}
          value={a.days(info.currentStreak)}
          note={info.playedDays > 0 ? ts.playedInTotal(a.playedDays(info.playedDays)) : null}
        />
      </div>

      {/* Pas de `data-nav-2d` : les lignes des deux colonnes ne sont pas alignées, la navigation
          spatiale sautait d'une colonne à l'autre. Haut et bas suivent l'ordre : top 10 puis genres. */}
      <div className="statpage__grid">
        <Card id="stats-hours" area="hours" title={ts.hours} source={LUDEX_SOURCE}>
          {info.historySessions > 0 ? (
            <MonthChart bars={hours} label={ts.hoursChart} format={formatDuration} />
          ) : (
            <p className="statpage__empty">{ts.hoursEmpty}</p>
          )}
        </Card>

        <Card id="stats-unlocks" area="unlocks" title={ts.unlocks} source={ts.unlocksSource(a.achievements(yearUnlocks))}>
          <MonthChart bars={unlocks} label={ts.unlocksChart} format={a.achievements} />
        </Card>

        <Card id="stats-top" area="top" title={ts.top} source={LIBRARY_SOURCE}>
          {top.length === 0 ? (
            <p className="statpage__empty">{ts.topEmpty}</p>
          ) : (
            <ol className="recap__top">
              {top.map((g, i) => (
                <li key={g.id}>
                  <button type="button" className="recap__row statpage__row" data-nav onClick={() => openGame(g)}>
                    <span className="recap__rank">{i + 1}</span>
                    <Cover game={g} className="hist__cover" />
                    <span className="recap__name">
                      <span title={g.title}>{g.title}</span>
                      <span className="recap__meter" aria-hidden="true">
                        <span style={{ width: `${((g.playtimeMinutes ?? 0) / maxTop) * 100}%` }} />
                      </span>
                    </span>
                    <span className="statpage__time">
                      <span className="recap__num">{formatPlaytime(g.playtimeMinutes)}</span>
                      <span className="recap__dim"><span className="sr-only">, </span>{sourceLabel(g)}</span>
                    </span>
                  </button>
                </li>
              ))}
            </ol>
          )}
        </Card>

        <Card id="stats-launchers" area="launchers" title={ts.launchers} source={ts.launchersSource}>
          {launchers.length === 0 ? (
            <p className="statpage__empty">{ts.noTime}</p>
          ) : (
            <ul className="statpage__shares">
              {launchers.map((s) => (
                <li key={s.key} className="statpage__share">
                  <LauncherIcon platform={s.key} size={16} />
                  <span className="recap__name">
                    <span>
                      {s.label}
                      <span className="recap__dim"><span className="sr-only">, </span>{a.games(s.games)}</span>
                    </span>
                    <span className="recap__meter" aria-hidden="true">
                      <span style={{ width: `${s.share * 100}%` }} />
                    </span>
                  </span>
                  <span className="recap__num">{percent.format(s.share)}</span>
                  <span className="recap__num recap__dim">{formatPlaytime(s.minutes)}</span>
                </li>
              ))}
            </ul>
          )}
        </Card>

        <Card
          id="stats-genres"
          area="genres"
          title={ts.genres}
          source={ts.genresSource(genres.withoutGenre > 0 ? a.games(genres.withoutGenre) : null)}
        >
          {genres.genres.length === 0 ? (
            <p className="statpage__empty">{ts.genresEmpty}</p>
          ) : (
            <ul className="statpage__shares">
              {genres.genres.map((s) => (
                <li key={s.key}>
                  <button
                    type="button"
                    className="statpage__share statpage__share--button"
                    data-nav
                    aria-label={ts.genreButton(s.label, a.games(s.games), s.minutes > 0 ? formatPlaytime(s.minutes) : null)}
                    onClick={() => showGenre(s.key)}
                  >
                    <span className="recap__name">
                      <span>
                        {s.label}
                        <span className="recap__dim">{a.games(s.games)}</span>
                      </span>
                      <span className="recap__meter">
                        <span style={{ width: `${s.share * 100}%` }} />
                      </span>
                    </span>
                    <span className="recap__num">{s.minutes > 0 ? formatPlaytime(s.minutes) : "·"}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </Card>
      </div>
    </section>
  );
}
