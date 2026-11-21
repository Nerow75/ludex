import { useEffect, useMemo, useState, type ReactNode } from "react";
import { loadStats, type StatsInfo } from "../lib/api";
import { formatDate, formatDuration, formatPlaytime } from "../lib/format";
import type { SectionContext } from "../lib/sections";
import { genreStats, launcherShares, monthBars, peak, playtimeTotals, sourceLabel, statsEmpty, topGames, type MonthBar, type PlaytimeTotals } from "../lib/stats";
import { parseDay } from "../lib/story";
import { Cover } from "./Cover";
import { LauncherIcon } from "./LauncherIcon";
import { EmptyState } from "./States";

const integer = new Intl.NumberFormat("fr-FR");
const percent = new Intl.NumberFormat("fr-FR", { style: "percent", maximumFractionDigits: 0 });
const plural = (n: number, one: string, many: string) => `${integer.format(n)} ${n > 1 ? many : one}`;
/** « 68 jeux, temps des launchers », « 60 par le launcher, 8 par Ludex ». */
function totalsNote(t: PlaytimeTotals): string {
  if (t.played === 0) return "Aucun jeu mesuré";
  if (t.fromLudex === 0) return `${plural(t.played, "jeu", "jeux")}, temps des launchers`;
  if (t.fromLauncher === 0) return `${plural(t.played, "jeu", "jeux")}, mesurés par Ludex`;
  return `${integer.format(t.fromLauncher)} par le launcher, ${integer.format(t.fromLudex)} par Ludex`;
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
  const summary = top ? `${label}. Mois le plus haut : ${top.title}, ${format(top.value)}` : `${label} : rien sur les 12 derniers mois`;
  return (
    <div className="recap__chart statpage__chart" role="img" aria-label={summary}>
      {bars.map((b, i) => (
        <div key={i} className={`recap__col${b.current ? " is-current" : ""}`} title={`${b.title} : ${b.value > 0 ? format(b.value) : "rien"}`}>
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
  const top = useMemo(() => topGames(games), [games]);
  const launchers = useMemo(() => launcherShares(games), [games]);
  const genres = useMemo(() => genreStats(games), [games]);

  const header = (
    <div className="bar">
      <h2 id="stats-title">Statistiques</h2>
    </div>
  );
  if (error) {
    return (
      <section aria-labelledby="stats-title">
        {header}
        <EmptyState title="Statistiques indisponibles">{error}</EmptyState>
      </section>
    );
  }
  if (!info) return <section aria-labelledby="stats-title">{header}</section>;
  if (statsEmpty(games, info)) {
    return (
      <section aria-labelledby="stats-title">
        {header}
        <EmptyState title="Pas encore de statistiques">
          Aucun temps de jeu connu : Steam et GOG Galaxy donnent le leur, et Ludex mesure les parties lancées depuis Ludex. Lance un jeu ici et cette page se remplira
        </EmptyState>
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
  const ludexSince = info.historySince ? `depuis le ${formatDate(info.historySince)}` : null;
  const LUDEX_SOURCE = `Parties lancées depuis Ludex${ludexSince ? `, mesurées ${ludexSince}` : ""}, au fuseau de ce PC.`;
  const LIBRARY_SOURCE = "Compteur de chaque jeu : celui du launcher (Steam, GOG Galaxy) s'il en a un, sinon le temps mesuré par Ludex. Jamais additionnés.";

  return (
    <section aria-labelledby="stats-title" className="statpage">
      {header}

      <div className="recap__stats statpage__tiles">
        <Tile
          label="Temps de jeu connu"
          value={totals.minutes > 0 ? formatPlaytime(totals.minutes) : "Aucun"}
          note={totalsNote(totals)}
        />
        <Tile
          label="Avec Ludex, 12 derniers mois"
          value={yearSecs > 0 ? formatDuration(yearSecs) : "0 min"}
          note={`${plural(yearSessions, "session", "sessions")}, ${plural(yearDays, "jour joué", "jours joués")}`}
        />
        <Tile
          label="Plus longue série"
          value={plural(info.bestStreak, "jour", "jours")}
          note={info.bestStreakEnd ? `Jusqu'au ${dayText(info.bestStreakEnd)}` : "Aucune partie mesurée par Ludex"}
        />
        <Tile
          label="Série en cours"
          value={plural(info.currentStreak, "jour", "jours")}
          note={info.playedDays > 0 ? `${plural(info.playedDays, "jour joué", "jours joués")} en tout` : null}
        />
      </div>

      <div className="statpage__grid" data-nav-2d>
        <Card id="stats-hours" area="hours" title="Heures par mois" source={LUDEX_SOURCE}>
          {info.historySessions > 0 ? (
            <MonthChart bars={hours} label="Temps de jeu mesuré par Ludex par mois" format={formatDuration} />
          ) : (
            <p className="statpage__empty">Aucune partie lancée depuis Ludex pour l'instant : seuls les temps des launchers sont connus.</p>
          )}
        </Card>

        <Card id="stats-unlocks" area="unlocks" title="Succès Steam par mois" source={`Dates de déblocage gardées par Steam : ${plural(yearUnlocks, "succès", "succès")} sur 12 mois.`}>
          <MonthChart bars={unlocks} label="Succès Steam débloqués par mois" format={(n) => plural(n, "succès", "succès")} />
        </Card>

        <Card id="stats-top" area="top" title="Les plus joués" source={LIBRARY_SOURCE}>
          {top.length === 0 ? (
            <p className="statpage__empty">Aucun jeu avec un temps de jeu connu.</p>
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

        <Card id="stats-launchers" area="launchers" title="Par launcher" source="Même compteur que « Les plus joués » : chaque jeu compte dans son launcher.">
          {launchers.length === 0 ? (
            <p className="statpage__empty">Aucun temps de jeu connu.</p>
          ) : (
            <ul className="statpage__shares">
              {launchers.map((s) => (
                <li key={s.key} className="statpage__share">
                  <LauncherIcon platform={s.key} size={16} />
                  <span className="recap__name">
                    <span>
                      {s.label}
                      <span className="recap__dim"><span className="sr-only">, </span>{plural(s.games, "jeu", "jeux")}</span>
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
          title="Par genre"
          source={`Genres des fiches, corrections comprises. Un jeu à plusieurs genres compte pour chacun.${genres.withoutGenre > 0 ? ` ${plural(genres.withoutGenre, "jeu", "jeux")} sans genre.` : ""}`}
        >
          {genres.genres.length === 0 ? (
            <p className="statpage__empty">Aucun jeu n'a de genre pour l'instant.</p>
          ) : (
            <ul className="statpage__shares">
              {genres.genres.map((s) => (
                <li key={s.key}>
                  <button
                    type="button"
                    className="statpage__share statpage__share--button"
                    data-nav
                    aria-label={`${s.label}, ${plural(s.games, "jeu", "jeux")}, ${s.minutes > 0 ? formatPlaytime(s.minutes) : "aucun temps connu"} : voir ces jeux`}
                    onClick={() => showGenre(s.key)}
                  >
                    <span className="recap__name">
                      <span>
                        {s.label}
                        <span className="recap__dim">{plural(s.games, "jeu", "jeux")}</span>
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
