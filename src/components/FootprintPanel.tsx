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
import { t } from "../lib/i18n";
import { actionHint, folderSize, footprintStatus, insideLabel, installDay, mainAction } from "../lib/footprint";

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
      {done ? t.fiche.footprint.copied : t.fiche.footprint.copy}
    </button>
  );
}

function FolderRow({ f, inside, onOpen }: { f: FootprintFolder; inside: string | null; onOpen: (item: string) => void }) {
  return (
    <li className={f.exists ? undefined : "is-gone"}>
      <div className="foot__what">
        <span className="foot__label">
          {f.label}
          {inside && <span className="foot__tag">{inside}</span>}
        </span>
        <span className="path" title={f.path}>{f.path}</span>
      </div>
      <span className="foot__size">{folderSize(f)}</span>
      {f.exists && (
        <button type="button" className="btn-ghost" data-nav onClick={() => onOpen(f.item)}>
          {f.file ? t.fiche.footprint.show : t.fiche.footprint.open}
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
    const timer = window.setTimeout(() => void load(false), present ? 0 : 300);
    return () => window.clearTimeout(timer);
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
        {error ? <p className="notice notice--error">{error}</p> : <p className="foot__status">{t.fiche.footprint.scanning(title)}</p>}
      </div>
    );
  }

  const action = mainAction(fp);
  const open = (item: string) => attempt(() => openFootprintItem(id, item), null);
  const hasUninstall = fp.uninstall.length > 0;

  return (
    <div className="foot" aria-label={t.fiche.footprint.title(fp.title)}>
      <div className="foot__head">
        <div>
          <h3>{t.fiche.footprint.title(fp.title)}</h3>
          <p className={`foot__status${!fp.installed && fp.leftovers > 0 ? " is-left" : ""}`} aria-live="polite">
            {footprintStatus(fp)}
          </p>
        </div>
        <button type="button" className="btn-ghost" data-nav disabled={busy} onClick={() => void load(true)}>
          {busy ? t.fiche.footprint.reading : fp.installed ? t.fiche.footprint.reread : t.fiche.footprint.leftovers}
        </button>
      </div>

      {error && <p className="notice notice--error">{error}</p>}
      {note && <p className="notice">{note}</p>}

      {fp.installed && (
        <div className="foot__acts">
          {action?.kind === "steam" && (
            <button type="button" className="btn-ghost" data-nav onClick={() => attempt(() => uninstallGame(id), t.fiche.footprint.steamDone(fp.title))}>
              {t.fiche.footprint.steam}
            </button>
          )}
          {action?.kind === "run" &&
            (confirm ? (
              <span className="saves__confirm" role="group" aria-label={t.fiche.footprint.confirm}>
                {t.fiche.footprint.runAsk(fp.title)}
                <button
                  type="button"
                  className="btn-ghost btn-ghost--danger"
                  data-nav
                  onClick={() => {
                    setConfirm(false);
                    void attempt(() => runUninstaller(id, action.item), t.fiche.footprint.runDone);
                  }}
                >
                  {t.fiche.footprint.run}
                </button>
                <button type="button" className="btn-ghost" data-nav onClick={() => setConfirm(false)}>
                  {t.fiche.footprint.cancel}
                </button>
              </span>
            ) : (
              <button type="button" className="btn-ghost" data-nav onClick={() => setConfirm(true)}>
                {t.fiche.footprint.runButton}
              </button>
            ))}
          {action?.kind === "settings" && (
            <button type="button" className="btn-ghost" data-nav onClick={() => attempt(openAppsSettings, null)}>
              {t.fiche.footprint.settings}
            </button>
          )}
          {action?.kind === "folder" && (
            <button type="button" className="btn-ghost" data-nav onClick={() => open("install")}>
              {t.fiche.footprint.folder}
            </button>
          )}
          <span className="foot__hint">{actionHint(fp, action)}</span>
        </div>
      )}

      <h4>{t.fiche.footprint.folders}</h4>
      <ul className="foot__list">
        {fp.folders.map((f) => <FolderRow key={f.item} f={f} inside={insideLabel(fp, f)} onOpen={open} />)}
      </ul>
      {fp.savesNote && <p className="foot__note">{t.fiche.footprint.savesNote(fp.savesNote)}</p>}

      <h4>{t.fiche.footprint.uninstall}</h4>
      {hasUninstall ? (
        <ul className="foot__list foot__list--entries">
          {fp.uninstall.map((u) => (
            <li key={u.item} className={u.exists ? undefined : "is-gone"}>
              <div className="foot__what">
                <span className="foot__label">
                  {u.exists ? u.name ?? t.fiche.footprint.unnamed : t.fiche.footprint.entryGone}
                  {u.exists && (u.publisher || u.version) && <span className="foot__tag">{[u.publisher, u.version].filter(Boolean).join(" · ")}</span>}
                </span>
                <span className="path" title={u.key}>{u.key}</span>
                {u.exists && (
                  <dl className="foot__facts">
                    {u.command && (
                      <>
                        <dt>{t.fiche.footprint.command}</dt>
                        <dd className="path" title={u.command}>{u.command}</dd>
                      </>
                    )}
                    {u.location && (
                      <>
                        <dt>{t.fiche.footprint.location}</dt>
                        <dd className="path" title={u.location}>{u.location}</dd>
                      </>
                    )}
                    {u.declaredSize !== null && (
                      <>
                        <dt>{t.fiche.footprint.declaredSize}</dt>
                        <dd>{formatSize(u.declaredSize)}</dd>
                      </>
                    )}
                    {u.installDate && (
                      <>
                        <dt>{t.fiche.footprint.installedOn}</dt>
                        <dd>{installDay(u.installDate)}</dd>
                      </>
                    )}
                    {u.refusal && (
                      <>
                        <dt>{t.fiche.footprint.uninstaller}</dt>
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
          {t.fiche.footprint.noEntry(fp.platform === "xbox")}
        </p>
      )}

      <h4>{t.fiche.footprint.registry}</h4>
      {fp.keys.length > 0 ? (
        <ul className="foot__list">
          {fp.keys.map((k) => (
            <li key={k.key} className={k.exists ? undefined : "is-gone"}>
              <div className="foot__what">
                <span className="foot__label">{k.label}</span>
                <span className="path" title={k.key}>{k.key}</span>
              </div>
              <span className="foot__size">{k.exists ? t.fiche.footprint.present : t.fiche.footprint.removed}</span>
              {k.exists && <CopyButton text={k.key} />}
            </li>
          ))}
        </ul>
      ) : (
        <p className="foot__note">{t.fiche.footprint.noKeys}</p>
      )}
      {fp.savesRegistry && <p className="foot__note">{t.fiche.footprint.savesRegistry}</p>}

      <p className="foot__note">{t.fiche.footprint.nothingDeleted}</p>
    </div>
  );
}
