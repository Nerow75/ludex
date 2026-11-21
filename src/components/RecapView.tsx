import { useCallback, useEffect, useMemo, useState } from "react";
import { recap as loadRecap, type Game, type Recap, type RecapPeriod } from "../lib/api";
import { formatDate, formatDuration } from "../lib/format";
import { formatNumber, locale, t } from "../lib/i18n";
import type { SectionContext } from "../lib/sections";
import { hasStory, parseDay, periodTitle } from "../lib/story";
import { Cover } from "./Cover";
import { ChevronLeftIcon, ChevronRightIcon } from "./Icons";
import { EmptyState } from "./States";
import { StoryView } from "./StoryView";

const PERIODS: RecapPeriod[] = ["week", "month", "year"];
/** En dessous, l'historique est trop court pour un récap parlant : une note l'explique. */
const FEW_SESSIONS = 3;

/** Formats de dates de la langue actuelle, gardés par locale. */
const dateFormats = new Map<string, { monthYear: Intl.DateTimeFormat; dayFull: Intl.DateTimeFormat; monthShort: Intl.DateTimeFormat }>();
function dates() {
  let f = dateFormats.get(locale());
  if (!f) {
    f = {
      monthYear: new Intl.DateTimeFormat(locale(), { month: "long", year: "numeric" }),
      dayFull: new Intl.DateTimeFormat(locale(), { weekday: "long", day: "numeric", month: "long" }),
      monthShort: new Intl.DateTimeFormat(locale(), { month: "short" }),
    };
    dateFormats.set(locale(), f);
  }
  return f;
}

const capitalize = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
/** `data-nav` seulement sur un bouton actif : la navigation au clavier et à la manette saute les autres. */
const nav = (enabled: boolean) => (enabled ? { "data-nav": "" } : {});

/** « +35 % par rapport à la semaine précédente (à la même date) ». `null` si rien à comparer. */
function comparison(r: Recap): string | null {
  const w = t.activity.period[r.period];
  const c = t.activity.compare;
  const current = r.back === 0 && r.elapsedDays < r.days.length;
  const prev = current ? r.previousToDateSecs : r.previousSecs;
  const when = current ? c.sameStage : "";
  if (prev === 0) return r.totalSecs > 0 ? c.nothing(w.previous, when) : null;
  const pct = Math.round(((r.totalSecs - prev) / prev) * 100);
  const sign = pct > 0 ? "+" : pct < 0 ? "−" : "";
  return `${c.change(`${sign}${formatNumber(Math.abs(pct))}`, w.versus, when)} (${formatDuration(prev)})`;
}

interface Bar {
  label: string;
  title: string;
  secs: number;
  future: boolean;
}

/** Barres de l'histogramme : une par jour, une par mois pour l'année. */
function bars(r: Recap): Bar[] {
  const start = parseDay(r.start);
  const f = dates();
  const day = (i: number) => new Date(start.getFullYear(), start.getMonth(), start.getDate() + i);
  if (r.period === "year") {
    const months: Bar[] = Array.from({ length: 12 }, (_, m) => {
      const d = new Date(start.getFullYear(), m, 1);
      return { label: f.monthShort.format(d).replace(".", ""), title: capitalize(f.monthYear.format(d)), secs: 0, future: true };
    });
    r.days.forEach((secs, i) => {
      const m = months[day(i).getMonth()];
      m.secs += secs;
      if (i < r.elapsedDays) m.future = false;
    });
    return months;
  }
  return r.days.map((secs, i) => {
    const d = day(i);
    const label = r.period === "week" ? t.activity.weekdaysShort[i] : i === 0 || (i + 1) % 5 === 0 ? String(d.getDate()) : "";
    return { label, title: capitalize(f.dayFull.format(d)), secs, future: i >= r.elapsedDays };
  });
}

function Histogram({ data }: { data: Bar[] }) {
  const max = Math.max(1, ...data.map((b) => b.secs));
  return (
    <div className="recap__chart" role="img" aria-label={t.activity.recap.chart}>
      {data.map((b, i) => (
        <div key={i} className={`recap__col${b.future ? " is-future" : ""}`} title={t.activity.barTitle(b.title, b.secs > 0 ? formatDuration(b.secs) : null)}>
          <div className="recap__track">
            <div className="recap__bar" style={{ height: `${b.secs > 0 ? Math.max(2, (b.secs / max) * 100) : 0}%` }} />
          </div>
          <span>{b.label}</span>
        </div>
      ))}
    </div>
  );
}

function Stat({ label, value, note, title }: { label: string; value: string; note?: string | null; title?: string }) {
  return (
    <div className="recap__stat" title={title}>
      <span>{label}</span>
      <b>{value}</b>
      {note && <small>{note}</small>}
    </div>
  );
}

/**
 * Section « Récap » : temps de jeu d'une semaine, d'un mois ou d'une année, calculé côté Rust depuis
 * l'historique des parties mesurées par Ludex. Le front n'envoie que la période et le recul.
 */
export function RecapView({ games, playing, openGame }: SectionContext) {
  const [period, setPeriod] = useState<RecapPeriod>("week");
  const [back, setBack] = useState(0);
  const [data, setData] = useState<Recap | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [story, setStory] = useState(false);
  const closeStory = useCallback(() => setStory(false), []);
  const byId = useMemo(() => new Map(games.map((g) => [g.id, g])), [games]);

  // Relu au changement de période et quand une partie commence ou se termine.
  useEffect(() => {
    let alive = true;
    loadRecap(period, back)
      .then((r) => {
        if (!alive) return;
        setData(r);
        setError(null);
      })
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [period, back, playing]);

  // Plus rien avant le début de l'historique : la flèche « précédente » s'arrête là.
  const canGoBack = !!data?.historySince && parseDay(data.start).getTime() / 1000 > data.historySince;
  const a = t.activity;
  const tr = a.recap;
  const choose = (p: RecapPeriod) => {
    setPeriod(p);
    setBack(0);
  };
  const title = (id: string) => byId.get(id)?.title ?? a.removedGame;
  const open = (id: string) => {
    const g = byId.get(id);
    if (g) openGame(g);
  };

  const header = (
    <div className="bar">
      <h2 id="recap-title">{tr.title}</h2>
      <div className="seg" role="group" aria-label={tr.periodGroup}>
        {PERIODS.map((p) => (
          <button key={p} type="button" data-nav aria-pressed={p === period} onClick={() => choose(p)}>
            {a.period[p].label}
          </button>
        ))}
      </div>
      <div className="recap__nav">
        <button type="button" className="recap__arrow" {...nav(canGoBack)} aria-label={tr.prevPeriod} disabled={!canGoBack} onClick={() => setBack((b) => b + 1)}>
          <ChevronLeftIcon />
        </button>
        <button type="button" className="recap__arrow" {...nav(back > 0)} aria-label={tr.nextPeriod} disabled={back === 0} onClick={() => setBack((b) => Math.max(0, b - 1))}>
          <ChevronRightIcon />
        </button>
      </div>
      {data && hasStory(data) && (
        <button type="button" className="btn-ghost recap__story" data-nav onClick={() => setStory(true)}>
          {tr.viewStory}
        </button>
      )}
    </div>
  );

  if (error) {
    return (
      <section aria-labelledby="recap-title">
        {header}
        <EmptyState title={tr.unavailable}>{error}</EmptyState>
      </section>
    );
  }
  if (!data) return <section aria-labelledby="recap-title">{header}</section>;
  if (data.historySessions === 0) {
    return (
      <section aria-labelledby="recap-title">
        {header}
        <EmptyState title={tr.emptyTitle}>{tr.emptyBody}</EmptyState>
      </section>
    );
  }

  const r = data;
  const compare = comparison(r);
  const longest = r.longest;
  const weekday = r.favoriteWeekday === null ? null : a.weekdays[r.favoriteWeekday];
  const moment = r.favoriteMoment;
  const maxTop = Math.max(1, ...r.top.map((g) => g.secs));

  return (
    <section aria-labelledby="recap-title" className="recap">
      {header}
      {story && <StoryView recap={r} byId={byId} onClose={closeStory} />}
      <p className="recap__period">
        <span>{r.back === 0 ? a.period[r.period].currentTitle : capitalize(periodTitle(r))}</span>
        {r.back === 0 && <span className="recap__dim">{periodTitle(r)}</span>}
      </p>
      {r.historySessions < FEW_SESSIONS && (
        <p className="recap__note">{tr.shortHistory(r.historySince ? formatDate(r.historySince) : null)}</p>
      )}

      <div className="recap__hero">
        <b>{r.totalSecs > 0 ? formatDuration(r.totalSecs) : a.zero}</b>
        <span>{r.totalSecs > 0 ? compare : tr.noneMeasured}</span>
      </div>

      <Histogram data={bars(r)} />

      <div className="recap__stats">
        <Stat label={tr.statSessions} value={formatNumber(r.sessions)} note={a.games(r.games)} />
        <Stat
          label={tr.statStreak}
          value={a.days(r.bestStreak)}
          note={r.back === 0 ? (r.currentStreak > 0 ? tr.streakRunning(a.days(r.currentStreak)) : tr.noStreak) : tr.longestOfPeriod}
        />
        <Stat
          label={tr.statDay}
          value={weekday ? capitalize(weekday) : tr.none}
          note={r.favoriteWeekday === null ? null : formatDuration(r.weekdaySecs[r.favoriteWeekday])}
        />
        <Stat
          label={tr.statMoment}
          value={moment === null ? tr.none : a.momentNames[moment]}
          note={moment === null ? null : a.momentHours[moment]}
        />
        <Stat
          label={tr.statLongest}
          value={longest ? formatDuration(longest.secs) : tr.noneFem}
          note={longest ? title(longest.gameId) : null}
          title={longest ? formatDate(longest.start) : undefined}
        />
        <Stat
          label={tr.statAchievements}
          value={formatNumber(r.achievements)}
          note={r.achievementGames.length ? r.achievementGames.slice(0, 2).map((u) => `${title(u.gameId)} (${u.count})`).join(", ") : tr.noneUnlocked}
        />
      </div>

      {r.top.length > 0 && (
        <>
          <h3 className="recap__h3">{tr.mostPlayed}</h3>
          <ol className="recap__top">
            {r.top.map((g, i) => {
              const game: Game | undefined = byId.get(g.gameId);
              return (
                <li key={g.gameId}>
                  <button type="button" className="recap__row" {...nav(!!game)} disabled={!game} onClick={() => open(g.gameId)}>
                    <span className="recap__rank">{i + 1}</span>
                    {game ? <Cover game={game} className="hist__cover" /> : <span className="hist__cover recap__nocover" />}
                    <span className="recap__name">
                      <span>{title(g.gameId)}</span>
                      <span className="recap__meter"><span style={{ width: `${(g.secs / maxTop) * 100}%` }} /></span>
                    </span>
                    <span className="recap__num">{formatDuration(g.secs)}</span>
                    <span className="recap__dim">{a.sessions(g.sessions)}</span>
                  </button>
                </li>
              );
            })}
          </ol>
        </>
      )}

      {r.newGames.length > 0 && (
        <>
          <h3 className="recap__h3">{tr.newGames}<span>{r.newGames.length}</span></h3>
          <div className="recap__chips">
            {r.newGames.map((g) => (
              <button key={g.gameId} type="button" className="recap__chip" {...nav(byId.has(g.gameId))} disabled={!byId.has(g.gameId)} onClick={() => open(g.gameId)} title={tr.firstPlayed(formatDate(g.firstPlayed))}>
                {title(g.gameId)}
              </button>
            ))}
          </div>
        </>
      )}
    </section>
  );
}
