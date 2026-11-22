import { useState } from "react";
import type { FpsSeries, PerfSeries, SessionPerf } from "../lib/api";
import { t } from "../lib/i18n";
import {
  METRIC_LABEL,
  areaPath,
  chartSegments,
  formatFps,
  formatMetric,
  fpsAverageText,
  fpsLowText,
  fpsNote,
  fpsScaleMax,
  fpsSummary,
  lacksGpu,
  linePath,
  perfHeadline,
  scaleMax,
  seriesSummary,
  sessionDate,
  sessionLength,
  statText,
} from "../lib/performance";

interface Props {
  /** Sessions mesurées, la plus récente d'abord (déjà filtrées par `shownSessions`). */
  sessions: SessionPerf[];
}

/** Repère interne du tracé : le SVG s'étire à la largeur de la carte. */
const W = 600;
const H = 120;

/** Une mesure : chiffres du jeu et du PC, puis les deux courbes. */
function MetricChart({ perf, series }: { perf: SessionPerf; series: PerfSeries }) {
  const max = scaleMax(series, perf);
  const game = chartSegments(perf, series.gamePoints, max, W, H);
  const pc = chartSegments(perf, series.pcPoints, max, W, H);
  const label = METRIC_LABEL[series.metric];
  return (
    <figure className="perf__card" data-nav tabIndex={-1} aria-label={seriesSummary(series, perf)}>
      <h3 className="perf__title">{label}</h3>
      <dl className="perf__stats" aria-hidden="true">
        <div>
          <dt><span className="perf__key is-game" />{t.fiche.perf.game}</dt>
          <dd>{statText(series.metric, series.game)}</dd>
        </div>
        <div>
          <dt><span className="perf__key is-pc" />{t.fiche.perf.pc}</dt>
          <dd>{statText(series.metric, series.pc)}</dd>
        </div>
      </dl>
      <div className="perf__plot" aria-hidden="true">
        <span className="perf__max">{formatMetric(series.metric, max)}</span>
        <svg className="perf__svg" viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" focusable="false">
          <line className="perf__grid" x1="0" x2={W} y1={H / 2} y2={H / 2} />
          <path className="perf__area" d={areaPath(game, H)} />
          <path className="perf__line is-pc" d={linePath(pc)} />
          <path className="perf__line is-game" d={linePath(game)} />
        </svg>
        <span className="perf__axis">
          <span>0</span>
          <span>{sessionLength(perf)}</span>
        </span>
      </div>
    </figure>
  );
}

/** Images par seconde : moyenne et 1 % low, chaque courbe une valeur par point. */
function FpsChart({ perf, fps }: { perf: SessionPerf; fps: FpsSeries }) {
  const max = fpsScaleMax(fps);
  const avg = chartSegments(perf, fps.points, max, W, H);
  const low = chartSegments(perf, fps.lowPoints, max, W, H);
  const p = t.fiche.perf.fps;
  return (
    <figure className="perf__card" data-nav tabIndex={-1} aria-label={fpsSummary(fps)}>
      <h3 className="perf__title">{p.title}</h3>
      <dl className="perf__stats" aria-hidden="true">
        <div>
          <dt><span className="perf__key is-game" />{p.averageKey}</dt>
          <dd>{fpsAverageText(fps)}</dd>
        </div>
        <div>
          <dt><span className="perf__key is-pc" />{p.lowKey}</dt>
          <dd>{fpsLowText(fps)}</dd>
        </div>
      </dl>
      <div className="perf__plot" aria-hidden="true">
        <span className="perf__max">{formatFps(max)}</span>
        <svg className="perf__svg" viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" focusable="false">
          <line className="perf__grid" x1="0" x2={W} y1={H / 2} y2={H / 2} />
          <path className="perf__area" d={areaPath(avg, H)} />
          <path className="perf__line is-pc" d={linePath(low)} />
          <path className="perf__line is-game" d={linePath(avg)} />
        </svg>
        <span className="perf__axis">
          <span>0</span>
          <span>{sessionLength(perf)}</span>
        </span>
      </div>
    </figure>
  );
}

/** Onglet « Performances » : images par seconde, processeur, mémoire et carte graphique des dernières parties. */
export function PerformancePanel({ sessions }: Props) {
  const [chosen, setChosen] = useState(0);
  const index = Math.min(chosen, sessions.length - 1);
  const perf = sessions[index];
  if (!perf) return null;
  return (
    <div className="perf">
      <p className="fiche__note">{perfHeadline(sessions)}</p>
      {sessions.length > 1 && (
        <div className="perf__picks" role="radiogroup" aria-label={t.fiche.perf.picks}>
          {sessions.map((s, i) => (
            <button
              key={s.start}
              type="button"
              role="radio"
              aria-checked={i === index}
              className="perf__pick"
              data-nav
              onClick={() => setChosen(i)}
            >
              <span>{sessionDate(s)}</span>
              <em>{sessionLength(s)}</em>
            </button>
          ))}
        </div>
      )}
      {sessions.length === 1 && (
        <p className="perf__when">
          {sessionDate(perf)}, {sessionLength(perf)}
        </p>
      )}
      <div className="perf__grid-list">
        {perf.fps && <FpsChart key={`${perf.start}-fps`} perf={perf} fps={perf.fps} />}
        {perf.series.map((s) => (
          <MetricChart key={`${perf.start}-${s.metric}`} perf={perf} series={s} />
        ))}
      </div>
      {fpsNote(perf) && <p className="fiche__note">{fpsNote(perf)}</p>}
      {lacksGpu(perf) && <p className="fiche__note">{t.fiche.perf.noGpu}</p>}
      <p className="fiche__note">{t.fiche.perf.noTemps}</p>
    </div>
  );
}
