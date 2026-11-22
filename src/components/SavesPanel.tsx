import { useCallback, useEffect, useState } from "react";
import { backupSaves, gameSaves, onSavesBackup, openSaveCopy, openSaveLocation, restoreSaves, type BackupReason, type GameSaves, type SavesCopy } from "../lib/api";
import { formatBytes, formatDateTime } from "../lib/format";
import { live, t } from "../lib/i18n";

const REASON: Record<BackupReason, string> = live(() => t.fiche.saves.reason);

const files = (n: number) => t.fiche.saves.files(n);

interface Props {
  gameId: string;
  /** Une partie est en cours : la restauration sera refusée côté Rust, et la liste relue à la fin. */
  playing?: boolean;
}

type Message = { text: string; error?: boolean } | null;

/** Sauvegardes d'un jeu : emplacements trouvés, copies, copie manuelle et restauration confirmée. */
export function SavesPanel({ gameId, playing = false }: Props) {
  const [saves, setSaves] = useState<GameSaves | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState<string | null>(null);
  const [message, setMessage] = useState<Message>(null);

  const reload = useCallback(() => {
    let alive = true;
    gameSaves(gameId)
      .then((s) => alive && setSaves(s))
      .catch((e) => alive && setMessage({ text: String(e), error: true }));
    return () => {
      alive = false;
    };
  }, [gameId]);

  useEffect(() => reload(), [reload, playing]);

  // Copie automatique de fin de partie : liste relue.
  useEffect(() => {
    const off = onSavesBackup((e) => {
      if (e.id === gameId) reload();
    });
    return () => {
      off.then((f) => f());
    };
  }, [gameId, reload]);

  const backup = async () => {
    setBusy(true);
    setMessage(null);
    try {
      const out = await backupSaves(gameId);
      setMessage({ text: out.unchanged ? t.fiche.saves.unchanged : t.fiche.saves.done(files(out.copy.files), formatBytes(out.copy.size)) });
      reload();
    } catch (e) {
      setMessage({ text: String(e), error: true });
    } finally {
      setBusy(false);
    }
  };

  const restore = async (copy: SavesCopy) => {
    setBusy(true);
    setConfirming(null);
    setMessage(null);
    try {
      const out = await restoreSaves(gameId, copy.id);
      const safety = out.safety ? t.fiche.saves.safety : "";
      setMessage({ text: t.fiche.saves.restored(formatDateTime(copy.created), files(out.restored), safety) });
      reload();
    } catch (e) {
      setMessage({ text: String(e), error: true });
    } finally {
      setBusy(false);
    }
  };

  const reveal = (open: Promise<void>) => open.catch((e) => setMessage({ text: String(e), error: true }));

  if (!saves) return <p className="saves__note">{message?.text ?? t.fiche.saves.searching}</p>;
  const known = saves.name !== null;
  return (
    <div className="saves">
      {saves.status === "loading" && !known && <p className="saves__note">{t.fiche.saves.loading}</p>}
      {saves.status === "unavailable" && !known && <p className="saves__note">{t.fiche.saves.unavailable}</p>}
      {saves.status === "ready" && !known && <p className="saves__note">{t.fiche.saves.unknown}</p>}

      {known && (
        <section>
          <h4>{t.fiche.saves.onThisPc}</h4>
          {saves.locations.length === 0 ? (
            <p className="saves__note">{t.fiche.saves.none}</p>
          ) : (
            <ul className="saves__list">
              {saves.locations.map((l) => (
                <li key={l.path}>
                  <span className="path" title={l.path}>{l.path}</span>
                  <span className="saves__meta">{files(l.files)}, {formatBytes(l.size)}</span>
                  <button type="button" className="btn-ghost" data-nav title={t.fiche.saves.openPath(l.path)} onClick={() => reveal(openSaveLocation(gameId, l.path))}>
                    {t.fiche.saves.open}
                  </button>
                </li>
              ))}
            </ul>
          )}
          {saves.registry && <p className="saves__note">{t.fiche.saves.registry}</p>}
        </section>
      )}

      {saves.copies.length > 0 && (
        <section>
          <h4>{t.fiche.saves.copies}</h4>
          <ul className="saves__list">
            {saves.copies.map((c) => (
              <li key={c.id}>
                <span>{formatDateTime(c.created)}</span>
                <span className="saves__meta">{REASON[c.reason]}, {files(c.files)}, {formatBytes(c.size)}</span>
                <button type="button" className="btn-ghost saves__open" data-nav title={t.fiche.saves.showCopy} onClick={() => reveal(openSaveCopy(gameId, c.id))}>
                  {t.fiche.saves.open}
                </button>
                {confirming === c.id ? (
                  <span className="saves__confirm">
                    <span>{t.fiche.saves.confirm}</span>
                    <button type="button" className="btn-ghost" disabled={busy} onClick={() => restore(c)}>{t.fiche.saves.restore}</button>
                    <button type="button" className="btn-ghost" onClick={() => setConfirming(null)}>{t.fiche.saves.cancel}</button>
                  </span>
                ) : (
                  <button
                    type="button"
                    className="btn-ghost"
                    disabled={busy || !known}
                    title={playing ? t.fiche.saves.closeGame : undefined}
                    onClick={() => setConfirming(c.id)}
                  >
                    {t.fiche.saves.restore}
                  </button>
                )}
              </li>
            ))}
          </ul>
        </section>
      )}

      <div className="saves__acts">
        {known && (
          <button type="button" className="btn-ghost" disabled={busy || saves.locations.length === 0} onClick={backup}>
            {t.fiche.saves.backupNow}
          </button>
        )}
        {message && <span className={message.error ? "saves__msg is-error" : "saves__msg"} role="status">{message.text}</span>}
      </div>
      <p className="saves__note">
        {t.fiche.saves.copiesIn} <span className="path" title={saves.dir}>{saves.dir}</span>
        {saves.auto ? t.fiche.saves.autoOn : t.fiche.saves.autoOff}
      </p>
    </div>
  );
}
