import { useCallback, useEffect, useRef, useState } from "react";
import {
  moveGame,
  moveGameCancel,
  moveGameDismiss,
  moveGameFinish,
  moveGameStatus,
  moveOptions,
  movePickFolder,
  onGameMove,
  openAppsSettings,
  openInstallDir,
  openSteamStorage,
  type MoveChoice,
  type MoveOptions,
  type MoveProgress,
  type MoveTarget,
} from "../lib/api";
import { formatSize } from "../lib/format";
import { firstUsable, launcherHelp, moveActive, movePercent, moveStatus, targetNote, targetUsable } from "../lib/gameMove";
import { t } from "../lib/i18n";

/** Suit le déplacement en cours (un seul à la fois, quel que soit le jeu). */
export function useMoveJob(): [MoveProgress | null, (job: MoveProgress | null) => void] {
  const [job, setJob] = useState<MoveProgress | null>(null);
  useEffect(() => {
    let alive = true;
    void moveGameStatus().then((j) => alive && setJob(j)).catch(() => {});
    const off = onGameMove((j) => alive && setJob(j));
    return () => {
      alive = false;
      void off.then((f) => f());
    };
  }, []);
  return [job, setJob];
}

/**
 * Progression d'un déplacement et ses boutons : Annuler pendant la copie, confirmation sur place
 * une fois la copie vérifiée (« Finir le déplacement » supprime l'ancien dossier), OK à la fin.
 */
export function MoveJobView({ job, onChange }: { job: MoveProgress; onChange: (job: MoveProgress | null) => void }) {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const finishRef = useRef<HTMLButtonElement>(null);
  const okRef = useRef<HTMLButtonElement>(null);
  const percent = movePercent(job);
  const m = t.fiche.move;

  // La confirmation prend le focus : c'est la seule chose à décider.
  useEffect(() => {
    if (job.phase === "ready") finishRef.current?.focus();
    if (job.phase === "done" || job.phase === "failed" || job.phase === "cancelled") okRef.current?.focus();
  }, [job.phase]);

  const run = async (task: () => Promise<MoveProgress | null | void>) => {
    setBusy(true);
    setError(null);
    try {
      const out = await task();
      if (out !== undefined) onChange(out);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const failed = job.phase === "failed";

  return (
    <div className="move__job" role="group" aria-label={m.running(job.title)}>
      <p className={`foot__status${failed ? " is-left" : ""}`} aria-live="polite">{moveStatus(job)}</p>
      {percent !== null && (
        <div className="move__bar" role="progressbar" aria-label={m.progress(job.title)} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent}>
          <span style={{ width: `${percent}%` }} />
        </div>
      )}
      {job.warnings.map((w) => <p key={w} className="foot__note move__warn">{w}</p>)}
      {error && <p className="notice notice--error">{error}</p>}
      <div className="foot__acts">
        {job.phase === "ready" && (
          <button ref={finishRef} type="button" className="btn-ghost btn-ghost--danger" data-nav disabled={busy} onClick={() => void run(moveGameFinish)}>
            {m.finish}
          </button>
        )}
        {(job.phase === "preparing" || job.phase === "copying" || job.phase === "verifying" || job.phase === "ready") && (
          <button type="button" className="btn-ghost" data-nav disabled={busy} onClick={() => void run(moveGameCancel)}>
            {m.cancel}
          </button>
        )}
        {!moveActive(job) && (
          <button
            ref={okRef}
            type="button"
            className="btn-ghost"
            data-nav
            onClick={() =>
              void run(async () => {
                await moveGameDismiss();
                return null;
              })
            }
          >
            {m.ok}
          </button>
        )}
      </div>
    </div>
  );
}

function TargetRow({ target, chosen, onChoose }: { target: MoveTarget; chosen: boolean; onChoose: () => void }) {
  const usable = targetUsable(target);
  return (
    <li className={usable ? undefined : "is-gone"}>
      <button
        type="button"
        role="radio"
        aria-checked={chosen}
        className={`move__target${chosen ? " is-on" : ""}`}
        data-nav
        disabled={!usable}
        onClick={onChoose}
      >
        <span className="foot__what">
          <span className="foot__label">
            <b>{target.letter}</b>
            {target.name && <span className="foot__tag">{target.name}</span>}
          </span>
          <span className="path" title={target.path}>{target.path}</span>
        </span>
        <span className="move__note">{targetNote(target)}</span>
      </button>
    </li>
  );
}

/**
 * Panneau « Déplacer vers un autre disque » d'un jeu (fiche et Stockage). Steam, Xbox et les autres
 * launchers : comment le faire chez eux. « Autres » et jeux ajoutés à la main : Ludex copie,
 * vérifie, puis demande confirmation avant de supprimer l'ancien dossier.
 */
export function MovePanel({ id, title }: { id: string; title: string }) {
  const [opts, setOpts] = useState<MoveOptions | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [choice, setChoice] = useState<MoveChoice | null>(null);
  const [busy, setBusy] = useState(false);
  const [job, setJob] = useMoveJob();
  const m = t.fiche.move;

  const load = useCallback(() => {
    return moveOptions(id)
      .then((o) => {
        setOpts(o);
        setError(null);
        setChoice((c) => {
          if (c?.kind === "picked" && o.picked) return c;
          if (c?.kind === "folder" && o.targets.some((x) => x.key === c.key && targetUsable(x))) return c;
          if (o.picked && targetUsable(o.picked)) return { kind: "picked" };
          const first = firstUsable(o.targets);
          return first ? { kind: "folder", key: first.key } : null;
        });
      })
      .catch((e) => setError(String(e)));
  }, [id]);
  useEffect(() => void load(), [load]);
  // Fin d'un déplacement (celui-ci ou un autre) : destinations et place relues.
  const ended = !moveActive(job);
  useEffect(() => {
    if (ended) void load();
  }, [ended, load]);

  const attempt = async (task: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await task();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  if (!opts) {
    return (
      <div className="foot move" aria-live="polite">
        {error ? <p className="notice notice--error">{error}</p> : <p className="foot__status">{m.loading}</p>}
      </div>
    );
  }

  const mine = job && job.id === opts.id ? job : null;
  const other = job && job.id !== opts.id && moveActive(job) ? job : null;
  const chosen = choice?.kind === "picked" ? opts.picked : opts.targets.find((x) => x.key === choice?.key) ?? null;
  const openFolder = opts.from ? (
    <button type="button" className="btn-ghost" data-nav onClick={() => void attempt(() => openInstallDir(opts.id))}>
      {m.folderButton}
    </button>
  ) : null;

  return (
    <div className="foot move" aria-label={m.title(title)}>
      <div className="foot__head">
        <div>
          <h3>{m.title(opts.title)}</h3>
          {opts.from && <p className="foot__status path" title={opts.from}>{m.current(opts.from, formatSize(opts.bytes))}</p>}
        </div>
      </div>
      {error && <p className="notice notice--error">{error}</p>}

      {opts.mode === "steam" && (
        <div className="foot__acts">
          <button type="button" className="btn-ghost" data-nav onClick={() => void attempt(openSteamStorage)}>{m.steamButton}</button>
          <span className="foot__hint">{m.steam}</span>
        </div>
      )}
      {opts.mode === "windows" && (
        <div className="foot__acts">
          <button type="button" className="btn-ghost" data-nav onClick={() => void attempt(openAppsSettings)}>{m.windowsButton}</button>
          <span className="foot__hint">{m.windows}</span>
        </div>
      )}
      {(opts.mode === "launcher" || opts.mode === "refused") && (
        <div className="foot__acts">
          {openFolder}
          <span className="foot__hint">{opts.mode === "launcher" ? launcherHelp(opts.platform) : opts.reason}</span>
        </div>
      )}

      {opts.mode === "ludex" &&
        (mine ? (
          <MoveJobView job={mine} onChange={setJob} />
        ) : (
          <>
            <p className="foot__note">{m.intro}</p>
            {other && <p className="notice">{m.otherGame(other.title)}</p>}
            <h4 id={`move-${opts.id}`}>{m.choose}</h4>
            {opts.targets.length === 0 && !opts.picked ? (
              <p className="foot__note">{m.noTarget}</p>
            ) : (
              <ul className="foot__list move__targets" role="radiogroup" aria-labelledby={`move-${opts.id}`}>
                {opts.targets.map((x) => (
                  <TargetRow key={x.key} target={x} chosen={choice?.kind === "folder" && choice.key === x.key} onChoose={() => setChoice({ kind: "folder", key: x.key })} />
                ))}
                {opts.picked && <TargetRow target={opts.picked} chosen={choice?.kind === "picked"} onChoose={() => setChoice({ kind: "picked" })} />}
              </ul>
            )}
            <div className="foot__acts">
              <button
                type="button"
                className="btn-ghost"
                data-nav
                disabled={busy || !!other || !chosen || !targetUsable(chosen)}
                onClick={() => choice && void attempt(() => moveGame(opts.id, choice).then(setJob))}
              >
                {m.start}
              </button>
              {opts.canPick && (
                <button
                  type="button"
                  className="btn-ghost"
                  data-nav
                  disabled={busy}
                  onClick={() =>
                    void attempt(() =>
                      movePickFolder(opts.id).then((o) => {
                        if (!o) return;
                        setOpts(o);
                        setChoice({ kind: "picked" });
                      }),
                    )
                  }
                >
                  {m.pick}
                </button>
              )}
              {openFolder}
            </div>
          </>
        ))}
    </div>
  );
}
