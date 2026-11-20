import { useCallback, useEffect, useState } from "react";
import {
  gameFootprint,
  openAppsSettings,
  openFootprintItem,
  runUninstaller,
  uninstallGame,
  type Footprint,
  type FootprintFolder,
} from "../lib/api";
import { formatSize } from "../lib/format";
import { actionHint, folderSize, footprintStatus, installDay, mainAction } from "../lib/footprint";

/** Copie un texte (clé du registre) : clic de l'utilisateur, aucune permission Tauri. */
function CopyButton({ text }: { text: string }) {
  const [done, setDone] = useState(false);
  return (
    <button
      type="button"
      className="btn-ghost foot__copy"
      data-nav
      onClick={() =>
        navigator.clipboard.writeText(text).then(
          () => {
            setDone(true);
            window.setTimeout(() => setDone(false), 1500);
          },
          () => setDone(false),
        )
      }
    >
      {done ? "Copiée" : "Copier"}
    </button>
  );
}

function FolderRow({ f, onOpen }: { f: FootprintFolder; onOpen: (item: string) => void }) {
  return (
    <li className={f.exists ? undefined : "is-gone"}>
      <div className="foot__what">
        <span className="foot__label">
          {f.label}
          {f.inInstall && <span className="foot__tag">dans le dossier du jeu</span>}
        </span>
        <span className="path" title={f.path}>{f.path}</span>
      </div>
      <span className="foot__size">{folderSize(f)}</span>
      {f.exists && (
        <button type="button" className="btn-ghost" data-nav onClick={() => onOpen(f.item)}>
          {f.file ? "Montrer" : "Ouvrir"}
        </button>
      )}
    </li>
  );
}

/**
 * Vue avancée du Stockage, avant de désinstaller : ce que le jeu laisse sur le PC (dossiers,
 * sauvegardes, caches, entrée de désinstallation de Windows, clés du registre), puis les restes une
 * fois le jeu parti de la bibliothèque. Lecture seule : Ludex ouvre des dossiers, la désinstallation
 * de Steam ou le désinstalleur officiel du jeu, et ne supprime rien.
 */
export function FootprintPanel({ id, title, present }: { id: string; title: string; present: boolean }) {
  const [fp, setFp] = useState<Footprint | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirm, setConfirm] = useState(false);

  const load = useCallback(
    (rescan: boolean) => {
      setBusy(true);
      return gameFootprint(id, rescan)
        .then((f) => {
          setFp(f);
          setError(null);
        })
        .catch((e) => setError(String(e)))
        .finally(() => setBusy(false));
    },
    [id],
  );

  // Relu quand le jeu quitte (ou retrouve) la bibliothèque : désinstallation vue par le rescan.
  useEffect(() => {
    const t = window.setTimeout(() => void load(false), present ? 0 : 300);
    return () => window.clearTimeout(t);
  }, [load, present]);

  const attempt = async (task: () => Promise<void>, done: string | null) => {
    try {
      await task();
      setNote(done);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  if (!fp) {
    return (
      <div className="foot" aria-live="polite">
        {error ? <p className="notice notice--error">{error}</p> : <p className="foot__status">Relevé de ce que {title} laisse sur le PC…</p>}
      </div>
    );
  }

  const action = mainAction(fp);
  const open = (item: string) => attempt(() => openFootprintItem(id, item), null);
  const hasUninstall = fp.uninstall.length > 0;

  return (
    <div className="foot" aria-label={`Ce que ${fp.title} laisse sur le PC`}>
      <div className="foot__head">
        <div>
          <h3>Ce que {fp.title} laisse sur le PC</h3>
          <p className={`foot__status${!fp.installed && fp.leftovers > 0 ? " is-left" : ""}`} aria-live="polite">
            {footprintStatus(fp)}
          </p>
        </div>
        <button type="button" className="btn-ghost" data-nav disabled={busy} onClick={() => void load(true)}>
          {busy ? "Lecture…" : fp.installed ? "Relire" : "Vérifier les restes"}
        </button>
      </div>

      {error && <p className="notice notice--error">{error}</p>}
      {note && <p className="notice">{note}</p>}

      {fp.installed && (
        <div className="foot__acts">
          {action?.kind === "steam" && (
            <button type="button" className="btn-ghost" data-nav onClick={() => attempt(() => uninstallGame(id), `Steam demande confirmation avant de désinstaller ${fp.title}. La vue montre ensuite ce qui reste.`)}>
              Désinstaller avec Steam
            </button>
          )}
          {action?.kind === "run" &&
            (confirm ? (
              <span className="saves__confirm" role="group" aria-label="Confirmer">
                Lancer le désinstalleur de {fp.title} ?
                <button
                  type="button"
                  className="btn-ghost btn-ghost--danger"
                  data-nav
                  onClick={() => {
                    setConfirm(false);
                    void attempt(() => runUninstaller(id, action.item), "Désinstalleur lancé. Quand il a fini, « Vérifier les restes » relit le PC.");
                  }}
                >
                  Lancer
                </button>
                <button type="button" className="btn-ghost" data-nav onClick={() => setConfirm(false)}>
                  Annuler
                </button>
              </span>
            ) : (
              <button type="button" className="btn-ghost" data-nav onClick={() => setConfirm(true)}>
                Lancer le désinstalleur
              </button>
            ))}
          {action?.kind === "settings" && (
            <button type="button" className="btn-ghost" data-nav onClick={() => attempt(openAppsSettings, null)}>
              Ouvrir les Paramètres de Windows
            </button>
          )}
          {action?.kind === "folder" && (
            <button type="button" className="btn-ghost" data-nav onClick={() => open("install")}>
              Ouvrir le dossier du jeu
            </button>
          )}
          <span className="foot__hint">{actionHint(fp, action)}</span>
        </div>
      )}

      <h4>Dossiers et fichiers</h4>
      <ul className="foot__list">
        {fp.folders.map((f) => <FolderRow key={f.item} f={f} onOpen={open} />)}
      </ul>
      {fp.savesNote && <p className="foot__note">Sauvegardes : {fp.savesNote}</p>}

      <h4>Désinstallation Windows</h4>
      {hasUninstall ? (
        <ul className="foot__list foot__list--entries">
          {fp.uninstall.map((u) => (
            <li key={u.item} className={u.exists ? undefined : "is-gone"}>
              <div className="foot__what">
                <span className="foot__label">
                  {u.exists ? u.name ?? "Sans nom" : "Entrée supprimée"}
                  {u.exists && (u.publisher || u.version) && <span className="foot__tag">{[u.publisher, u.version].filter(Boolean).join(" · ")}</span>}
                </span>
                <span className="path" title={u.key}>{u.key}</span>
                {u.exists && (
                  <dl className="foot__facts">
                    {u.command && (
                      <>
                        <dt>Commande</dt>
                        <dd className="path" title={u.command}>{u.command}</dd>
                      </>
                    )}
                    {u.location && (
                      <>
                        <dt>Emplacement</dt>
                        <dd className="path" title={u.location}>{u.location}</dd>
                      </>
                    )}
                    {u.declaredSize !== null && (
                      <>
                        <dt>Taille déclarée</dt>
                        <dd>{formatSize(u.declaredSize)}</dd>
                      </>
                    )}
                    {u.installDate && (
                      <>
                        <dt>Installé le</dt>
                        <dd>{installDay(u.installDate)}</dd>
                      </>
                    )}
                    {u.refusal && (
                      <>
                        <dt>Désinstalleur</dt>
                        <dd>{u.refusal}</dd>
                      </>
                    )}
                  </dl>
                )}
              </div>
              {u.exists && <CopyButton text={u.key} />}
            </li>
          ))}
        </ul>
      ) : (
        <p className="foot__note">
          Aucune entrée pour ce jeu dans « Applications installées » de Windows
          {fp.platform === "xbox" ? " : les jeux du Microsoft Store y figurent sous un autre format." : "."}
        </p>
      )}

      <h4>Registre</h4>
      {fp.keys.length > 0 ? (
        <ul className="foot__list">
          {fp.keys.map((k) => (
            <li key={k.key} className={k.exists ? undefined : "is-gone"}>
              <div className="foot__what">
                <span className="foot__label">{k.label}</span>
                <span className="path" title={k.key}>{k.key}</span>
              </div>
              <span className="foot__size">{k.exists ? "Présente" : "Supprimée"}</span>
              {k.exists && <CopyButton text={k.key} />}
            </li>
          ))}
        </ul>
      ) : (
        <p className="foot__note">Aucune autre clé connue de Ludex pour ce jeu.</p>
      )}
      {fp.savesRegistry && <p className="foot__note">La base des sauvegardes signale aussi des réglages de ce jeu dans le registre (emplacement exact non relevé).</p>}

      <p className="foot__note">Ludex ne supprime rien : il montre et ouvre. Un reste se supprime à la main, depuis l'Explorateur ou l'Éditeur du Registre.</p>
    </div>
  );
}
