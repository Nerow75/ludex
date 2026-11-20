// Choix de l'emplacement des données lourdes (cache des images, base et copies des sauvegardes),
// dans l'écran d'accueil et les Paramètres. Le front ne transmet jamais de chemin : un disque de la
// liste (point de montage relu côté Rust), le dossier choisi par le sélecteur natif (gardé côté
// Rust), ou l'emplacement d'origine.
import { useEffect, useState } from "react";
import { chooseDataDir, dataLocation, moveData, type DataLocation, type DataTarget, type MoveDone } from "../lib/api";
import { bytesLabel, currentText, diskOptions, heavyBytes, moveText } from "../lib/dataPlace";

interface Props {
  /** Données déplacées : bibliothèque relue. */
  onMoved?: (done: MoveDone) => void;
  /** Montre aussi ce qui reste dans %APPDATA% et pourquoi. */
  explainKept?: boolean;
}

/** Choix en attente de confirmation, avec le dossier qui serait utilisé. */
type Choice = { target: DataTarget; root: string } | null;

export function DataPlace({ onMoved, explainKept }: Props) {
  const [loc, setLoc] = useState<DataLocation | null>(null);
  const [choice, setChoice] = useState<Choice>(null);
  const [busy, setBusy] = useState<"pick" | "move" | null>(null);
  const [msg, setMsg] = useState<{ text: string; error: boolean } | null>(null);

  useEffect(() => {
    dataLocation()
      .then(setLoc)
      .catch((e) => setMsg({ text: String(e), error: true }));
  }, []);

  if (!loc) return <p className="place__note">{msg ? msg.text : "Mesure des données de Ludex…"}</p>;

  const options = diskOptions(loc);
  const choose = (next: Choice) => {
    setMsg(null);
    setChoice(next);
  };
  const pick = () => {
    setBusy("pick");
    setMsg(null);
    chooseDataDir()
      .then((root) => root && setChoice({ target: { kind: "picked" }, root }))
      .catch((e) => setMsg({ text: String(e), error: true }))
      .finally(() => setBusy(null));
  };
  const move = () => {
    if (!choice) return;
    if (loc.preview) {
      setMsg({ text: "Aperçu de l'écran d'accueil : rien n'est déplacé.", error: false });
      setChoice(null);
      return;
    }
    setBusy("move");
    setMsg(null);
    moveData(choice.target)
      .then((done) => {
        setLoc(done.location);
        setChoice(null);
        const warn = done.warnings.length ? ` ${done.warnings.join(" ")}` : "";
        setMsg({ text: `${bytesLabel(done.moved)} déplacés vers ${done.location.root || "l'emplacement d'origine"}.${warn}`, error: false });
        onMoved?.(done);
      })
      .catch((e) => setMsg({ text: String(e), error: true }))
      .finally(() => setBusy(null));
  };
  const original = { cache: loc.defaultCache, copies: loc.defaultCopies };

  return (
    <div className="place">
      <dl className="place__now">
        <div>
          <dt>Cache des images et base des sauvegardes</dt>
          <dd>
            <span className="place__size">{bytesLabel(loc.cacheBytes)}</span>
            <code title={loc.cacheDir}>{loc.cacheDir}</code>
          </dd>
        </div>
        <div>
          <dt>Copies de sauvegardes{loc.copiesSeparate && " (dossier choisi à part)"}</dt>
          <dd>
            <span className="place__size">{bytesLabel(loc.copiesBytes)}</span>
            <code title={loc.copiesDir}>{loc.copiesDir}</code>
          </dd>
        </div>
      </dl>
      <p className={`place__note ${loc.missing ? "is-warning" : ""}`}>{currentText(loc)}</p>

      <div className="place__disks" role="radiogroup" aria-label="Disque des données de Ludex">
        {options.map((o) => {
          const picked = choice?.target.kind === "drive" && choice.target.mount === o.disk.mount;
          return (
            <button
              key={o.disk.mount}
              type="button"
              role="radio"
              aria-checked={picked}
              className={`place__disk ${picked ? "is-picked" : ""} ${o.low ? "is-low" : ""}`}
              data-nav
              disabled={!o.selectable || busy !== null}
              title={!o.fits ? `Pas assez de place : ${bytesLabel(heavyBytes(loc))} à déplacer` : undefined}
              onClick={() => choose({ target: { kind: "drive", mount: o.disk.mount }, root: o.disk.target })}
            >
              <span className="place__disk-head">
                <b>{o.label}</b>
                {o.tags.map((t) => <span key={t} className="place__tag">{t}</span>)}
              </span>
              <span className="place__bar" aria-hidden="true"><span style={{ width: `${Math.round(o.used * 100)}%` }} /></span>
              <small>{o.fits ? o.detail : `${o.detail}, pas assez de place`}</small>
            </button>
          );
        })}
      </div>

      <div className="place__acts">
        <button type="button" className="btn-ghost set__small" data-nav disabled={busy !== null} onClick={pick}>
          {busy === "pick" ? "Choix…" : "Autre dossier…"}
        </button>
        {loc.root && (
          <button
            type="button"
            className="btn-ghost set__small"
            data-nav
            disabled={busy !== null}
            onClick={() => choose({ target: { kind: "default" }, root: "" })}
          >
            Revenir à l'emplacement d'origine
          </button>
        )}
      </div>

      {choice && (
        <div className="place__confirm" role="group" aria-label="Confirmer le déplacement">
          <p>
            {choice.root
              ? moveText(loc, choice.root)
              : `${bytesLabel(heavyBytes(loc))} à déplacer. Cache dans ${original.cache}.${loc.copiesSeparate ? "" : ` Copies de sauvegardes dans ${original.copies}.`}`}{" "}
            Copie, vérification, puis suppression de l'ancien dossier. Impossible pendant une partie.
          </p>
          <div className="place__acts">
            <button type="button" className="btn-ghost set__small is-primary" data-nav autoFocus disabled={busy !== null} onClick={move}>
              {busy === "move" ? "Déplacement…" : choice.root ? `Déplacer vers ${choice.root}` : "Revenir à l'origine"}
            </button>
            <button type="button" className="btn-ghost set__small" data-nav disabled={busy !== null} onClick={() => choose(null)}>Annuler</button>
          </div>
        </div>
      )}

      {msg && <p className={`place__msg ${msg.error ? "is-error" : ""}`} role="status">{msg.text}</p>}
      {explainKept && (
        <p className="place__note">
          Tes préférences et ton historique ({bytesLabel(loc.keptBytes)}) restent dans <code>{loc.keptDir}</code> : ils sont petits, Ludex les lit dès
          le démarrage, et ils ne dépendent pas d'un disque qu'on peut débrancher.
        </p>
      )}
    </div>
  );
}
