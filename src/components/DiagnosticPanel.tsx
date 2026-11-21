// Diagnostic des launchers (bas des Paramètres) : état de chaque source au dernier scan, chemins et
// clés lus, jeux écartés, services en ligne, versions. « Copier le rapport » met dans le presse-papiers
// le texte anonymisé préparé par Rust (`diagnostic.rs`), pour déboguer un PC qui n'est pas le sien.
import { useCallback, useEffect, useState } from "react";
import { diagnostic as getDiagnostic, scanLibrary, type DiagSource, type Diagnostic, type ScanResult } from "../lib/api";
import { problems, serviceState, sourceDetail, sourceState, sourceTone } from "../lib/diagnostic";
import { discordState } from "../lib/discord";
import { relativeTime } from "../lib/format";
import { ChevronRightIcon } from "./Icons";

interface Props {
  /** Dernier scan connu du front : le diagnostic est relu quand il change (premier scan fini, rescan). */
  scan: ScanResult | null;
  /** Bibliothèque relue par « Relire ». */
  onScan: (scan: ScanResult) => void;
}

function SourceRow({ source, open, onToggle }: { source: DiagSource; open: boolean; onToggle: () => void }) {
  const tone = sourceTone(source);
  const detail = sourceDetail(source);
  const body = `diag-${source.id}`;
  return (
    <li className={`diag__src is-${tone}`}>
      <button type="button" className="diag__head" data-nav aria-expanded={open} aria-controls={body} onClick={onToggle}>
        <span className="diag__dot" aria-hidden="true" />
        <span className="diag__name">
          <span>{source.label}</span>
          {detail && <small>{detail}</small>}
        </span>
        <span className="diag__state">{sourceState(source)}</span>
        <span className="diag__chev" aria-hidden="true"><ChevronRightIcon /></span>
      </button>
      {open && (
        <div className="diag__body" id={body}>
          {source.locations.length > 0 && (
            <>
              <h4>Emplacements lus</h4>
              <ul className="diag__list">
                {source.locations.map((l) => (
                  <li key={`${l.label}-${l.path}`}>
                    <span className={`diag__found ${l.found ? "is-found" : ""}`}>{l.found ? "Trouvé" : "Absent"}</span>
                    <span className="diag__label">{l.label}</span>
                    <span className="path" title={l.path}>{l.path}</span>
                  </li>
                ))}
              </ul>
            </>
          )}
          {source.skipped.length > 0 && (
            <>
              <h4>{source.id === "manual" ? "Exécutables introuvables" : "Jeux écartés"}</h4>
              <ul className="diag__list">
                {source.skipped.map((k, i) => (
                  <li key={`${k.title}-${i}`}>
                    <span className="diag__label">{k.title}</span>
                    <span className="diag__why">{k.reason}</span>
                  </li>
                ))}
              </ul>
            </>
          )}
          {source.tools.length > 0 && (
            <>
              <h4>Masqués d'office (applis)</h4>
              <p className="diag__why">{source.tools.join(", ")}</p>
            </>
          )}
          {source.error && <p className="diag__why is-error">{source.error}</p>}
          {!source.locations.length && !source.skipped.length && !source.tools.length && !source.error && <p className="diag__why">Rien à signaler.</p>}
        </div>
      )}
    </li>
  );
}

export function DiagnosticPanel({ scan, onScan }: Props) {
  const [diag, setDiag] = useState<Diagnostic | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState<Set<string>>(() => new Set());
  const [rereading, setRereading] = useState(false);
  const [copied, setCopied] = useState<{ text: string; error: boolean } | null>(null);
  const [showText, setShowText] = useState(false);

  const load = useCallback(() => {
    getDiagnostic()
      .then((d) => {
        setDiag(d);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  }, []);

  // Relu à chaque scan : ouvert pendant le premier scan, le panneau ne reste pas sur « Pas encore lu ».
  useEffect(load, [load, scan]);

  const reread = () => {
    setRereading(true);
    setCopied(null);
    scanLibrary()
      .then(onScan) // le nouveau scan relit le diagnostic
      .catch((e) => setError(String(e)))
      .finally(() => setRereading(false));
  };

  const copy = () => {
    if (!diag) return;
    navigator.clipboard
      .writeText(diag.report)
      .then(() => setCopied({ text: "Rapport copié. Nom d'utilisateur et compte Steam remplacés, aucun titre perso.", error: false }))
      .catch(() => {
        // Presse-papiers refusé : le texte reste sélectionnable à la main.
        setShowText(true);
        setCopied({ text: "Copie impossible : sélectionne le texte ci-dessous.", error: true });
      });
  };

  const toggle = (id: string) =>
    setOpen((s) => {
      const next = new Set(s);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  if (!diag) {
    return error ? <p className="set__msg is-error" role="alert">{error}</p> : <p className="set__note">Lecture du diagnostic…</p>;
  }
  const count = problems(diag);

  return (
    <div className="diag">
      <div className="set__row">
        <span className="set__text">
          <span>{count ? `${count} problème${count > 1 ? "s" : ""} à regarder` : "Aucune erreur"}</span>
          <small>
            Ludex {diag.version} · {diag.windows} · WebView2 {diag.webview ?? "version inconnue"}
            {diag.scannedAt ? ` · dernier scan ${relativeTime(diag.scannedAt)}` : " · pas encore de scan"}
          </small>
        </span>
        <div className="set__acts">
          <button type="button" className="btn-ghost set__small" data-nav disabled={rereading} onClick={reread}>
            {rereading ? "Lecture…" : "Relire"}
          </button>
          <button type="button" className="btn-ghost set__small is-primary" data-nav onClick={copy}>Copier le rapport</button>
        </div>
      </div>
      {error && <p className="set__msg is-error" role="alert">{error}</p>}
      {copied && <p className={`set__msg ${copied.error ? "is-error" : ""}`} role="status">{copied.text}</p>}
      {showText && <textarea className="diag__text" readOnly value={diag.report} aria-label="Rapport de diagnostic" onFocus={(e) => e.currentTarget.select()} />}

      <ul className="diag__srcs" aria-label="Launchers">
        {diag.sources.map((s) => (
          <SourceRow key={s.id} source={s} open={open.has(s.id)} onToggle={() => toggle(s.id)} />
        ))}
      </ul>

      <h4 className="diag__title">Services en ligne</h4>
      <ul className="diag__srcs" aria-label="Services en ligne">
        {diag.services.map((s) => {
          const st = serviceState(s);
          return (
            <li key={s.id} className={`diag__src is-${st.tone}`}>
              <div className="diag__head is-static">
                <span className="diag__dot" aria-hidden="true" />
                <span className="diag__name">
                  <span>{s.label}</span>
                  <small>{s.host}</small>
                </span>
                <span className="diag__state" title={st.text}>{st.text}</span>
              </div>
            </li>
          );
        })}
      </ul>
      <h4 className="diag__title">Applications de ce PC</h4>
      <ul className="diag__srcs" aria-label="Applications de ce PC">
        {(() => {
          const st = discordState(diag.discord);
          return (
            <li className={`diag__src is-${st.tone}`}>
              <div className="diag__head is-static">
                <span className="diag__dot" aria-hidden="true" />
                <span className="diag__name">
                  <span>Statut Discord</span>
                  <small>client Discord de ce PC, aucun réseau</small>
                </span>
                <span className="diag__state" title={st.text}>{st.text}</span>
              </div>
            </li>
          );
        })()}
      </ul>
      <p className="set__note">
        Données : <span className="path" title={diag.dataDir}>{diag.dataDir}</span>
      </p>
    </div>
  );
}
