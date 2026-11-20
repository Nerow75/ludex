import { useCallback, useEffect, useState } from "react";
import { backupSaves, gameSaves, onSavesBackup, openSaveCopy, openSaveLocation, restoreSaves, type BackupReason, type GameSaves, type SavesCopy } from "../lib/api";
import { formatBytes, formatDateTime } from "../lib/format";

const REASON: Record<BackupReason, string> = {
  auto: "fin de partie",
  manual: "manuelle",
  before_restore: "avant restauration",
};

const files = (n: number) => `${n} fichier${n > 1 ? "s" : ""}`;

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
      setMessage({ text: out.unchanged ? "Rien n'a changé depuis la dernière copie." : `Copie faite : ${files(out.copy.files)}, ${formatBytes(out.copy.size)}.` });
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
      const safety = out.safety ? " L'état remplacé a été copié juste avant." : "";
      setMessage({ text: `Copie du ${formatDateTime(copy.created)} restaurée (${files(out.restored)}).${safety}` });
      reload();
    } catch (e) {
      setMessage({ text: String(e), error: true });
    } finally {
      setBusy(false);
    }
  };

  const reveal = (open: Promise<void>) => open.catch((e) => setMessage({ text: String(e), error: true }));

  if (!saves) return <p className="saves__note">{message?.text ?? "Recherche des sauvegardes…"}</p>;
  const known = saves.name !== null;
  return (
    <div className="saves">
      {saves.status === "loading" && !known && <p className="saves__note">Base des sauvegardes en cours de téléchargement.</p>}
      {saves.status === "unavailable" && !known && <p className="saves__note">Base des sauvegardes indisponible : vérifie la connexion.</p>}
      {saves.status === "ready" && !known && <p className="saves__note">Emplacement des sauvegardes inconnu pour ce jeu.</p>}

      {known && (
        <section>
          <h4>Sur ce PC</h4>
          {saves.locations.length === 0 ? (
            <p className="saves__note">Aucune sauvegarde trouvée.</p>
          ) : (
            <ul className="saves__list">
              {saves.locations.map((l) => (
                <li key={l.path}>
                  <span className="path" title={l.path}>{l.path}</span>
                  <span className="saves__meta">{files(l.files)}, {formatBytes(l.size)}</span>
                  <button type="button" className="btn-ghost" data-nav title={`Ouvrir ${l.path}`} onClick={() => reveal(openSaveLocation(gameId, l.path))}>
                    Ouvrir
                  </button>
                </li>
              ))}
            </ul>
          )}
          {saves.registry && <p className="saves__note">Une partie des données est dans le registre Windows : elle n'est pas copiée.</p>}
        </section>
      )}

      {saves.copies.length > 0 && (
        <section>
          <h4>Copies</h4>
          <ul className="saves__list">
            {saves.copies.map((c) => (
              <li key={c.id}>
                <span>{formatDateTime(c.created)}</span>
                <span className="saves__meta">{REASON[c.reason]}, {files(c.files)}, {formatBytes(c.size)}</span>
                <button type="button" className="btn-ghost saves__open" data-nav title="Montrer la copie dans l'Explorateur" onClick={() => reveal(openSaveCopy(gameId, c.id))}>
                  Ouvrir
                </button>
                {confirming === c.id ? (
                  <span className="saves__confirm">
                    <span>Remplacer les sauvegardes actuelles ? Elles seront copiées avant.</span>
                    <button type="button" className="btn-ghost" disabled={busy} onClick={() => restore(c)}>Restaurer</button>
                    <button type="button" className="btn-ghost" onClick={() => setConfirming(null)}>Annuler</button>
                  </span>
                ) : (
                  <button
                    type="button"
                    className="btn-ghost"
                    disabled={busy || !known}
                    title={playing ? "Ferme le jeu avant de restaurer" : undefined}
                    onClick={() => setConfirming(c.id)}
                  >
                    Restaurer
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
            Sauvegarder maintenant
          </button>
        )}
        {message && <span className={message.error ? "saves__msg is-error" : "saves__msg"} role="status">{message.text}</span>}
      </div>
      <p className="saves__note">
        Copies dans <span className="path" title={saves.dir}>{saves.dir}</span>
        {saves.auto ? ", à la fin de chaque partie lancée depuis Ludex." : ". Copie automatique désactivée."}
      </p>
    </div>
  );
}
