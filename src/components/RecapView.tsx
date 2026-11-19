import { useEffect, useMemo, useState } from "react";
import { recap as loadRecap, type Game, type Recap, type RecapPeriod } from "../lib/api";
import { formatDate, formatDuration } from "../lib/format";
import type { SectionContext } from "../lib/sections";
import { Cover } from "./Cover";
import { ChevronLeftIcon, ChevronRightIcon } from "./Icons";
import { EmptyState } from "./States";

const PERIODS: { id: RecapPeriod; label: string; current: string; previous: string }[] = [
  { id: "week", label: "Semaine", current: "Cette semaine", previous: "la semaine précédente" },
  { id: "month", label: "Mois", current: "Ce mois-ci", previous: "le mois précédent" },
  { id: "year", label: "Année", current: "Cette année", previous: "l'année précédente" },
];
const WEEKDAYS = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
const MOMENTS = ["la nuit", "le matin", "l'après-midi", "le soir"];
const MOMENT_HOURS = ["0 h à 6 h", "6 h à 12 h", "12 h à 18 h", "18 h à 24 h"];
/** En dessous, l'historique est trop court pour un récap parlant : une note l'explique. */
const FEW_SESSIONS = 3;

const integer = new Intl.NumberFormat("fr-FR");
const monthYear = new Intl.DateTimeFormat("fr-FR", { month: "long", year: "numeric" });
const dayMonth = new Intl.DateTimeFormat("fr-FR", { day: "numeric", month: "long" });
const dayFull = new Intl.DateTimeFormat("fr-FR", { weekday: "long", day: "numeric", month: "long" });
const monthShort = new Intl.DateTimeFormat("fr-FR", { month: "short" });

/** « AAAA-MM-JJ » en date locale (sans passer par UTC). */
const parseDay = (s: string) => {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, m - 1, d);
};
const capitalize = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
/** `data-nav` seulement sur un bouton actif : la navigation au clavier et à la manette saute les autres. */
const nav = (enabled: boolean) => (enabled ? { "data-nav": "" } : {});
const plural = (n: number, one: string, many: string) => `${integer.format(n)} ${n > 1 ? many : one}`;

/** « 16 au 22 novembre 2026 », « Novembre 2026 », « 2026 ». */
function periodTitle(r: Recap): string {
  const start = parseDay(r.start);
  const end = parseDay(r.end);
  if (r.period === "year") return String(start.getFullYear());
  if (r.period === "month") return capitalize(monthYear.format(start));
  if (start.getFullYear() !== end.getFullYear()) return `${dayMonth.format(start)} ${start.getFullYear()} au ${dayMonth.format(end)} ${end.getFullYear()}`;
  if (start.getMonth() !== end.getMonth()) return `${dayMonth.format(start)} au ${dayMonth.format(end)} ${end.getFullYear()}`;
  return `${start.getDate()} au ${dayMonth.format(end)} ${end.getFullYear()}`;
}

/** « +35 % par rapport à la semaine précédente (à la même date) ». `null` si rien à comparer. */
function comparison(r: Recap, previousLabel: string): string | null {
  const current = r.back === 0 && r.elapsedDays < r.days.length;
  const prev = current ? r.previousToDateSecs : r.previousSecs;
  const when = current ? ` au même stade` : "";
  if (prev === 0) return r.totalSecs > 0 ? `Rien de mesuré ${previousLabel}${when}` : null;
  const pct = Math.round(((r.totalSecs - prev) / prev) * 100);
  const sign = pct > 0 ? "+" : pct < 0 ? "−" : "";
  return `${sign}${integer.format(Math.abs(pct))} % par rapport à ${previousLabel}${when} (${formatDuration(prev)})`;
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
  const day = (i: number) => new Date(start.getFullYear(), start.getMonth(), start.getDate() + i);
  if (r.period === "year") {
    const months: Bar[] = Array.from({ length: 12 }, (_, m) => {
      const d = new Date(start.getFullYear(), m, 1);
      return { label: monthShort.format(d).replace(".", ""), title: capitalize(monthYear.format(d)), secs: 0, future: true };
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
    const label = r.period === "week" ? WEEKDAYS[i].slice(0, 3) : i === 0 || (i + 1) % 5 === 0 ? String(d.getDate()) : "";
    return { label, title: capitalize(dayFull.format(d)), secs, future: i >= r.elapsedDays };
  });
}

function Histogram({ data }: { data: Bar[] }) {
  const max = Math.max(1, ...data.map((b) => b.secs));
  return (
    <div className="recap__chart" role="img" aria-label="Temps de jeu par jour">
      {data.map((b, i) => (
        <div key={i} className={`recap__col${b.future ? " is-future" : ""}`} title={`${b.title} : ${b.secs > 0 ? formatDuration(b.secs) : "rien"}`}>
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
  const meta = PERIODS.find((p) => p.id === period) ?? PERIODS[0];
  const choose = (p: RecapPeriod) => {
    setPeriod(p);
    setBack(0);
  };
  const title = (id: string) => byId.get(id)?.title ?? "Jeu retiré de la bibliothèque";
  const open = (id: string) => {
    const g = byId.get(id);
    if (g) openGame(g);
  };

  const header = (
    <div className="bar">
      <h2 id="recap-title">Récap</h2>
      <div className="seg" role="group" aria-label="Période">
        {PERIODS.map((p) => (
          <button key={p.id} type="button" data-nav aria-pressed={p.id === period} onClick={() => choose(p.id)}>
            {p.label}
          </button>
        ))}
      </div>
      <div className="recap__nav">
        <button type="button" className="recap__arrow" {...nav(canGoBack)} aria-label="Période précédente" disabled={!canGoBack} onClick={() => setBack((b) => b + 1)}>
          <ChevronLeftIcon />
        </button>
        <button type="button" className="recap__arrow" {...nav(back > 0)} aria-label="Période suivante" disabled={back === 0} onClick={() => setBack((b) => Math.max(0, b - 1))}>
          <ChevronRightIcon />
        </button>
      </div>
    </div>
  );

  if (error) {
    return (
      <section aria-labelledby="recap-title">
        {header}
        <EmptyState title="Récap indisponible">{error}</EmptyState>
      </section>
    );
  }
  if (!data) return <section aria-labelledby="recap-title">{header}</section>;
  if (data.historySessions === 0) {
    return (
      <section aria-labelledby="recap-title">
        {header}
        <EmptyState title="Pas encore de récap">
          Ludex mesure les parties lancées depuis Ludex : lance un jeu ici et ton récap se remplira au fil des sessions
        </EmptyState>
      </section>
    );
  }

  const r = data;
  const compare = comparison(r, meta.previous);
  const longest = r.longest;
  const weekday = r.favoriteWeekday === null ? null : WEEKDAYS[r.favoriteWeekday];
  const moment = r.favoriteMoment;
  const maxTop = Math.max(1, ...r.top.map((g) => g.secs));

  return (
    <section aria-labelledby="recap-title" className="recap">
      {header}
      <p className="recap__period">
        <span>{r.back === 0 ? meta.current : capitalize(periodTitle(r))}</span>
        {r.back === 0 && <span className="recap__dim">{periodTitle(r)}</span>}
      </p>
      {r.historySessions < FEW_SESSIONS && (
        <p className="recap__note">
          Historique encore court : Ludex ne mesure que les parties lancées depuis Ludex
          {r.historySince ? `, depuis le ${formatDate(r.historySince)}` : ""}.
        </p>
      )}

      <div className="recap__hero">
        <b>{r.totalSecs > 0 ? formatDuration(r.totalSecs) : "0 min"}</b>
        <span>{r.totalSecs > 0 ? compare : "Aucune partie mesurée sur cette période"}</span>
      </div>

      <Histogram data={bars(r)} />

      <div className="recap__stats">
        <Stat label="Sessions" value={integer.format(r.sessions)} note={plural(r.games, "jeu", "jeux")} />
        <Stat
          label="Série de jours"
          value={plural(r.bestStreak, "jour", "jours")}
          note={r.back === 0 ? (r.currentStreak > 0 ? `${plural(r.currentStreak, "jour", "jours")} en cours` : "Aucune série en cours") : "La plus longue de la période"}
        />
        <Stat
          label="Jour préféré"
          value={weekday ? capitalize(weekday) : "Aucun"}
          note={r.favoriteWeekday === null ? null : formatDuration(r.weekdaySecs[r.favoriteWeekday])}
        />
        <Stat
          label="Moment préféré"
          value={moment === null ? "Aucun" : capitalize(MOMENTS[moment])}
          note={moment === null ? null : MOMENT_HOURS[moment]}
        />
        <Stat
          label="Plus longue session"
          value={longest ? formatDuration(longest.secs) : "Aucune"}
          note={longest ? title(longest.gameId) : null}
          title={longest ? formatDate(longest.start) : undefined}
        />
        <Stat
          label="Succès Steam"
          value={integer.format(r.achievements)}
          note={r.achievementGames.length ? r.achievementGames.slice(0, 2).map((u) => `${title(u.gameId)} (${u.count})`).join(", ") : "Aucun débloqué"}
        />
      </div>

      {r.top.length > 0 && (
        <>
          <h3 className="recap__h3">Les plus joués</h3>
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
                    <span className="recap__dim">{plural(g.sessions, "session", "sessions")}</span>
                  </button>
                </li>
              );
            })}
          </ol>
        </>
      )}

      {r.newGames.length > 0 && (
        <>
          <h3 className="recap__h3">Nouveaux jeux<span>{r.newGames.length}</span></h3>
          <div className="recap__chips">
            {r.newGames.map((g) => (
              <button key={g.gameId} type="button" className="recap__chip" {...nav(byId.has(g.gameId))} disabled={!byId.has(g.gameId)} onClick={() => open(g.gameId)} title={`Première partie le ${formatDate(g.firstPlayed)}`}>
                {title(g.gameId)}
              </button>
            ))}
          </div>
        </>
      )}
    </section>
  );
}
