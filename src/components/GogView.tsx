import { useCallback, useEffect, useState } from "react";
import {
  gogInstall,
  gogInstallCancel,
  gogInstallDismiss,
  gogInstallPause,
  gogInstallResume,
  gogLibrary,
  onGogAccount,
  onGogInstall,
  type GogGame,
  type GogLibrary,
  type GogProgress,
} from "../lib/api";
import { gogPercent, gogProgressText, jobActive, librarySummary } from "../lib/gog";
import { t } from "../lib/i18n";
import type { SectionContext } from "../lib/sections";
import { EmptyState } from "./States";

/** Vignette 16:9 de GOG ; cadre vide si absente ou si elle ne se charge pas. */
function Tile({ url }: { url: string | null }) {
  const [failed, setFailed] = useState(false);
  if (!url || failed) return <span className="wish__img gog__img" aria-hidden="true" />;
  return <img className="wish__img gog__img" src={url} alt="" loading="lazy" decoding="async" onError={() => setFailed(true)} />;
}

/** Progression de l'installation en cours (ou finie) sous la ligne du jeu. */
function JobBar({ job, ctx, onDismiss, onRetry }: { job: GogProgress; ctx: SectionContext; onDismiss: () => void; onRetry: () => void }) {
  const g = t.library.gog;
  const percent = gogPercent(job);
  const active = jobActive(job);
  const game = job.gameId ? ctx.games.find((x) => x.id === job.gameId) : undefined;
  const failed = job.phase === "failed";
  return (
    <div className={`gog__job is-${job.phase}`}>
      {(job.phase === "downloading" || job.phase === "paused") && (
        <div
          className="gog__bar"
          role="progressbar"
          aria-label={g.downloading(null)}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={percent ?? undefined}
        >
          <span style={{ width: `${percent ?? 0}%` }} />
        </div>
      )}
      <p className="gog__state" role={failed ? "alert" : "status"}>{gogProgressText(job)}</p>
      <div className="gog__actions">
        {job.phase === "downloading" && (
          <button type="button" className="btn-ghost" data-nav onClick={() => gogInstallPause(job.id).catch(() => {})}>{g.pause}</button>
        )}
        {job.phase === "paused" && (
          <button type="button" className="btn-ghost" data-nav onClick={() => gogInstallResume(job.id).catch(() => {})}>{g.resume}</button>
        )}
        {active && job.phase !== "installing" && job.phase !== "verifying" && (
          <button type="button" className="btn-ghost" data-nav onClick={() => gogInstallCancel(job.id).catch(() => {})}>{g.cancel}</button>
        )}
        {game && job.phase === "done" && (
          <button type="button" className="btn-ghost" data-nav onClick={() => ctx.openGame(game)}>{g.openGame}</button>
        )}
        {failed && (
          <button type="button" className="btn-ghost" data-nav onClick={onRetry}>{g.retry}</button>
        )}
        {!active && (
          <button type="button" className="btn-ghost" data-nav onClick={onDismiss}>{g.dismiss}</button>
        )}
      </div>
    </div>
  );
}

/**
 * Section « Pas installés » : jeux GOG possédés qui ne sont pas sur ce PC (compte GOG facultatif,
 * connecté dans les Paramètres). « Installer » ouvre le sélecteur de dossier côté Rust ; le front
 * n'envoie que l'id numérique du jeu.
 */
export function GogView(ctx: SectionContext) {
  const [lib, setLib] = useState<GogLibrary | null>(null);
  const [job, setJob] = useState<GogProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const g = t.library.gog;
  // Un jeu installé ou désinstallé change la liste.
  const gogCount = ctx.games.filter((x) => x.platform === "gog").length;

  const load = useCallback((refresh: boolean) => {
    setBusy(true);
    return gogLibrary(refresh)
      .then((l) => {
        setLib(l);
        setJob(l.job);
        setError(l.error);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setBusy(false));
  }, []);

  useEffect(() => {
    load(false);
  }, [load, gogCount]);

  useEffect(() => {
    const offAccount = onGogAccount(() => load(false));
    const offInstall = onGogInstall((p) => setJob(p));
    return () => {
      offAccount.then((stop) => stop()).catch(() => {});
      offInstall.then((stop) => stop()).catch(() => {});
    };
  }, [load]);

  const install = async (game: GogGame) => {
    setError(null);
    try {
      await gogInstall(game.id);
    } catch (e) {
      setError(String(e));
    }
  };

  const dismiss = (id: number) => {
    gogInstallDismiss(id).catch(() => {});
    setJob(null);
  };

  const running = jobActive(job);
  const state = lib?.account.state;
  return (
    <section className="wishes gog" aria-label={g.title}>
      <div className="bar">
        <h2>
          {g.title}
          {lib && state === "connected" && (
            <span>
              <span className="sr-only">, </span>
              {lib.games.length}
            </span>
          )}
        </h2>
        {state === "connected" && (
          <button type="button" className="btn-ghost" data-nav disabled={busy} onClick={() => load(true)}>
            {busy ? g.refreshing : g.refresh}
          </button>
        )}
      </div>

      {!lib && !error && <p className="wishes__note" role="status">{g.loading}</p>}
      {error && <p className="wishes__msg" role="alert">{error}</p>}

      {lib && state !== "connected" && (
        <EmptyState
          title={state === "expired" ? g.expiredTitle : g.offTitle}
          action={
            <button type="button" className="btn-ghost" data-nav onClick={ctx.openSettings}>
              {g.openSettings}
            </button>
          }
        >
          {state === "expired" ? g.expiredBody : g.offBody}
        </EmptyState>
      )}

      {lib && state === "connected" && (
        <>
          <p className="wishes__note">
            {librarySummary(lib)} {g.installHint}
          </p>
          {job && !lib.games.some((x) => x.id === job.id) && (
            <div className="wish gog__orphan">
              <div className="wish__text">
                <span className="wish__title">{job.title}</span>
              </div>
              <JobBar job={job} ctx={ctx} onDismiss={() => dismiss(job.id)} onRetry={() => install({ id: job.id, title: job.title, imageUrl: null })} />
            </div>
          )}
          {lib.games.length === 0 && !job && <EmptyState title={g.emptyTitle}>{g.emptyBody}</EmptyState>}
          {lib.games.length > 0 && (
            <ul className="wishes__list" aria-label={g.subtitle}>
              {lib.games.map((game) => {
                const mine = job?.id === game.id ? job : null;
                return (
                  <li key={game.id} className="wish gog__row">
                    <div className="wish__main">
                      <Tile url={game.imageUrl} />
                      <div className="wish__text">
                        <span className="wish__title">{game.title}</span>
                      </div>
                    </div>
                    <div className="wish__actions">
                      <button
                        type="button"
                        className="btn-ghost"
                        data-nav
                        disabled={running}
                        title={running && !mine ? g.busy : undefined}
                        onClick={() => install(game)}
                        aria-label={g.installLabel(game.title)}
                      >
                        {g.install}
                      </button>
                    </div>
                    {mine && <JobBar job={mine} ctx={ctx} onDismiss={() => dismiss(mine.id)} onRetry={() => install(game)} />}
                  </li>
                );
              })}
            </ul>
          )}
        </>
      )}
    </section>
  );
}
