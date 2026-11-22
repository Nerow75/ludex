import { useEffect, useState } from "react";
import { addRomDir, emulationInfo, removeRomDir, setEmulatorChoice, type EmulationInfo, type EmulationUpdate, type ScanResult } from "../lib/api";
import { emulatorSummary, folderRows, systemRows } from "../lib/emulation";
import { t } from "../lib/i18n";
import { LauncherIcon } from "./LauncherIcon";

/**
 * Section « Émulation » des Paramètres : émulateurs trouvés, dossiers de ROMs (ajout par le
 * sélecteur natif côté Rust, retrait d'un dossier choisi) et émulateur par console. Chaque
 * changement rescanne la bibliothèque (`onScan`).
 */
export function EmulationPanel({ scan, onScan, onToast }: { scan: ScanResult | null; onScan: (scan: ScanResult) => void; onToast: (title: string) => void }) {
  const [info, setInfo] = useState<EmulationInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const tx = t.settings.emulation;

  // Relu à chaque scan : émulateurs et dossiers viennent du dernier scan complet.
  useEffect(() => {
    let alive = true;
    emulationInfo()
      .then((i) => alive && setInfo(i))
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [scan]);

  const run = (call: () => Promise<EmulationUpdate | null>, done: string) => {
    setBusy(true);
    setError(null);
    call()
      .then((u) => {
        if (!u) return;
        setInfo(u.info);
        onScan(u.scan);
        onToast(done);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setBusy(false));
  };

  const folders = info ? folderRows(info) : [];
  const systems = info ? systemRows(info) : [];
  return (
    <section className="set__group" aria-labelledby="set-emulation">
      <h3 id="set-emulation">{tx.title}</h3>
      <p className="set__note">{tx.note}</p>
      <div className="set__row">
        <span className="set__text">
          <span className="with-icon"><LauncherIcon platform="emulation" size={16} />{info && !info.scanned ? tx.notScanned : info ? emulatorSummary(info) : "…"}</span>
          {info?.emulators.map((e) => (
            <small key={e.path} className="path" title={e.path}>{e.path}</small>
          ))}
        </span>
      </div>

      <p className="set__sub" role="heading" aria-level={4}>{tx.folders}</p>
      <p className="set__note">{tx.foldersHelp}</p>
      {info && folders.length === 0 && <p className="set__note">{tx.noFolder}</p>}
      {folders.map((f) => (
        <div className="set__row" key={f.path}>
          <span className="set__text">
            <span className="path" title={f.path}>{f.path}</span>
            <small>{f.found ? f.origin : `${f.origin}, ${tx.missing}`}</small>
          </span>
          {f.removable && (
            <button
              type="button"
              className="btn-ghost set__small"
              data-nav
              disabled={busy}
              aria-label={tx.removeLabel(f.path)}
              onClick={() => run(() => removeRomDir(f.path), tx.removed)}
            >
              {tx.remove}
            </button>
          )}
        </div>
      ))}
      <div className="set__row">
        <span />
        <button type="button" className="btn-ghost set__small" data-nav disabled={busy} onClick={() => run(addRomDir, tx.added)}>{tx.addFolder}</button>
      </div>

      {systems.length > 0 && (
        <>
          <p className="set__sub" role="heading" aria-level={4}>{tx.choices}</p>
          <p className="set__note">{tx.choicesHelp}</p>
          {systems.map((s) => (
            <div className="set__row" key={s.id}>
              <span className="set__text">
                <span>{s.name}</span>
                <small>{tx.games(s.games)}</small>
              </span>
              <select
                className="pick"
                data-nav
                disabled={busy}
                aria-label={tx.chooseLabel(s.name)}
                value={s.chosen ?? ""}
                onChange={(e) => run(() => setEmulatorChoice(s.id, e.target.value || null), tx.chosen(s.name))}
              >
                <option value="">{tx.automatic}</option>
                {s.emulators.map((e) => (
                  <option key={e.key} value={e.key}>{e.name}</option>
                ))}
              </select>
            </div>
          ))}
        </>
      )}
      {error && <p className="set__msg is-error" role="alert">{error}</p>}
    </section>
  );
}
