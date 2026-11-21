// Accueil du premier lancement, en quatre étapes : ce que Ludex a trouvé, où ranger ses données
// lourdes, quelques réglages, trois raccourcis. Puis la bibliothèque.
import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  setKeepInTray,
  setLaunchAtStartup,
  setTheme,
  settings as getSettings,
  THEME_LABEL,
  THEMES,
  type MoveDone,
  type ScanResult,
  type Theme,
  type WelcomeInfo,
} from "../lib/api";
import { applyTheme } from "../lib/theme";
import { positionText } from "../lib/a11y";
import { STEPS, stepAfter, summarize } from "../lib/welcome";
import { DataPlace } from "./DataPlace";
import { LudexMark } from "./Icons";
import { Switch } from "./SettingsView";

interface Props {
  scan: ScanResult;
  info: WelcomeInfo;
  /** Une manette est branchée : ses boutons accompagnent les touches. */
  pad: boolean;
  /** « C'est parti », Échap ou B. */
  onDone: () => void;
  /** « Voir les applis masquées ». */
  onShowHidden: () => void;
  /** Données lourdes déplacées : bibliothèque relue. */
  onScan: (scan: ScanResult) => void;
}

/** Durée du fondu de sortie, avant de rendre la main à la bibliothèque. */
const LEAVE_MS = 160;

const plural = (n: number, one: string, many: string) => `${n.toLocaleString("fr-FR")} ${n > 1 ? many : one}`;

/** Réglages de l'étape 3 (lus côté Rust, simulés dans l'aperçu). */
interface Prefs {
  launchAtStartup: boolean;
  keepInTray: boolean;
  theme: Theme;
}

export default function WelcomeScreen({ scan, info, pad, onDone, onShowHidden, onScan }: Props) {
  const s = useMemo(() => summarize(scan), [scan]);
  const root = useRef<HTMLDivElement>(null);
  const next = useRef<HTMLButtonElement>(null);
  const [step, setStep] = useState(0);
  const [leaving, setLeaving] = useState(false);
  const [prefs, setPrefs] = useState<Prefs | null>(null);
  const [prefsError, setPrefsError] = useState<string | null>(null);
  const live = useRef({ onDone, leaving });
  live.current = { onDone, leaving };

  const leave = (then: () => void) => {
    if (live.current.leaving) return;
    setLeaving(true);
    window.setTimeout(then, LEAVE_MS);
  };
  const leaveRef = useRef(leave);
  leaveRef.current = leave;

  useEffect(() => {
    getSettings()
      .then((st) => setPrefs({ launchAtStartup: st.launchAtStartup, keepInTray: st.keepInTray, theme: st.theme }))
      .catch((e) => setPrefsError(String(e)));
  }, []);

  // À chaque étape, le bouton principal reprend le focus (Entrée = continuer).
  useEffect(() => {
    next.current?.focus({ preventScroll: true });
    root.current?.scrollTo({ top: 0 });
  }, [step]);

  // Toutes les touches s'arrêtent ici (phase de capture) : aucun raccourci de la bibliothèque
  // derrière, taper ne remplit pas la recherche. La manette arrive par des `keydown` relayés.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      e.stopPropagation();
      const items = [...(root.current?.querySelectorAll<HTMLElement>("[data-nav]:not(:disabled)") ?? [])];
      const current = document.activeElement as HTMLElement | null;
      const i = current ? items.indexOf(current) : -1;
      if (e.key === "Enter" || (e.key === " " && i >= 0)) {
        e.preventDefault();
        (i >= 0 ? items[i] : next.current)?.click();
      } else if (e.key === "Escape") {
        e.preventDefault();
        leaveRef.current(live.current.onDone);
      } else if (e.key === "Tab" || e.key.startsWith("Arrow")) {
        e.preventDefault(); // le focus reste dans l'écran
        const back = e.key === "ArrowUp" || e.key === "ArrowLeft" || (e.key === "Tab" && e.shiftKey);
        const to = i < 0 ? Math.max(0, items.indexOf(next.current!)) : (i + (back ? -1 : 1) + items.length) % items.length;
        items[to]?.focus();
      } else if (e.key === "F5" || e.ctrlKey || e.metaKey) {
        e.preventDefault(); // ni rechargement de la WebView, ni raccourci du navigateur
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  /** Réglage changé : enregistré côté Rust, sauf dans l'aperçu. */
  const change = (patch: Partial<Prefs>, run: () => Promise<unknown>) => {
    setPrefs((p) => p && { ...p, ...patch });
    if (info.preview) return;
    run().catch((e) => setPrefsError(String(e)));
  };
  const changeTheme = (theme: Theme) => {
    if (!info.preview) applyTheme(theme);
    change({ theme }, () => setTheme(theme));
  };

  const others = [
    s.empty.length > 0 && `Installés sans jeu : ${s.empty.join(", ")}`,
    s.missing.length > 0 && `Pas sur ce PC : ${s.missing.join(", ")}`,
    s.failed.length > 0 && `Lecture impossible : ${s.failed.join(", ")}`,
  ].filter(Boolean) as string[];

  const last = step === STEPS.length - 1;
  const forward = () => {
    const to = stepAfter(step, 1);
    if (to === null) leave(onDone);
    else setStep(to);
  };

  return createPortal(
    <div
      ref={root}
      className={`welcome ${leaving ? "is-leaving" : ""}`}
      role="dialog"
      aria-modal="true"
      aria-labelledby="welcome-title"
      // Un portail remonte les événements React à ses parents : ils s'arrêtent ici.
      onClick={(e) => e.stopPropagation()}
      onMouseDown={(e) => e.stopPropagation()}
    >
      <div className="welcome__panel">
        <div className="welcome__top">
          <span className="welcome__brand"><LudexMark size={32} />Ludex</span>
          <ol className="welcome__steps" aria-label="Étapes">
            {STEPS.map((label, i) => (
              <li key={label} className={i === step ? "is-current" : i < step ? "is-done" : ""} aria-current={i === step ? "step" : undefined}>
                <span className="sr-only">{label}</span>
              </li>
            ))}
          </ol>
        </div>

        {/* Changement d'étape annoncé (le focus passe au bouton principal, qui ne dit pas où l'on est). */}
        <p className="sr-only" aria-live="polite">{positionText("Étape", step, STEPS.length)} : {STEPS[step]}</p>
        <div className="welcome__step" key={step}>
          {step === 0 && (
            <>
              <header className="welcome__head">
                <h1 id="welcome-title">
                  {s.total ? <><b>{plural(s.total, "jeu", "jeux")}</b> {s.where}</> : "Aucun jeu trouvé pour l'instant"}
                </h1>
                {s.total === 0 && (
                  <p className="welcome__lead">Installe un jeu avec ton launcher habituel, Ludex le verra tout seul. Un jeu rangé ailleurs s'ajoute avec son exécutable.</p>
                )}
              </header>

              {s.found.length > 0 && (
                <ul className="welcome__sources" aria-label="Jeux par source">
                  {s.found.map((f) => (
                    <li key={f.platform}>{f.label}<span>{f.count.toLocaleString("fr-FR")}</span></li>
                  ))}
                </ul>
              )}

              <div className="welcome__facts">
                {others.map((line) => <p key={line}>{line}</p>)}
                {s.tools > 0 && (
                  <p>
                    {plural(s.tools, "appli masquée", "applis masquées")} : ce ne sont pas des jeux (fonds d'écran, éditeurs, serveurs).{" "}
                    <button type="button" className="welcome__link" data-nav onClick={() => leave(onShowHidden)}>
                      {s.tools > 1 ? "Les voir" : "La voir"}
                    </button>
                  </p>
                )}
                {info.folders.length > 0 ? (
                  <p>
                    {info.folders.length > 1 ? "Dossiers de jeux lus" : "Dossier de jeux lu"} :{" "}
                    {info.folders.map((f, i) => <span key={f}>{i > 0 && ", "}<code>{f}</code></span>)}
                  </p>
                ) : (
                  <p>Aucun dossier « Jeux » ou « Games » à la racine de tes disques : un jeu rangé ailleurs s'ajoute avec <kbd>Ctrl</kbd><kbd>N</kbd>.</p>
                )}
              </div>
            </>
          )}

          {step === 1 && (
            <>
              <header className="welcome__head">
                <h1 id="welcome-title">Où ranger les données de Ludex ?</h1>
                <p className="welcome__lead">
                  Ludex garde les images des jeux et la base des sauvegardes, et copie tes sauvegardes à la fin des parties. Tout va sur le disque de Windows
                  par défaut ; un autre disque évite de le remplir. Modifiable plus tard dans les Paramètres.
                </p>
              </header>
              <DataPlace explainKept onMoved={(done: MoveDone) => onScan(done.scan)} />
            </>
          )}

          {step === 2 && (
            <>
              <header className="welcome__head">
                <h1 id="welcome-title">Quelques réglages</h1>
                <p className="welcome__lead">Tous modifiables plus tard dans les Paramètres (<kbd>Ctrl</kbd><kbd>,</kbd>).</p>
              </header>
              {!prefs ? (
                <p className="place__note">{prefsError ?? "Lecture des réglages…"}</p>
              ) : (
                <div className="welcome__prefs">
                  <Switch
                    label="Lancer avec Windows"
                    detail="Ludex démarre caché dans la zone de notification."
                    checked={prefs.launchAtStartup}
                    onChange={(v) => change({ launchAtStartup: v }, () => setLaunchAtStartup(v))}
                  />
                  <Switch
                    label="Garder Ludex dans la zone de notification à la fermeture"
                    detail="Le temps des jeux Epic, Ubisoft, Rockstar, Xbox, Riot, Amazon Games et itch.io continue d'être mesuré."
                    checked={prefs.keepInTray}
                    onChange={(v) => change({ keepInTray: v }, () => setKeepInTray(v))}
                  />
                  <div className="set__row set__row--stack">
                    <span className="set__text">
                      <span>Thème</span>
                      <small>« Comme Windows » suit le mode clair ou sombre de Windows.</small>
                    </span>
                    <div className="seg" role="radiogroup" aria-label="Thème">
                      {THEMES.map((t) => (
                        <button key={t} type="button" role="radio" aria-checked={prefs.theme === t} data-nav onClick={() => changeTheme(t)}>
                          {THEME_LABEL[t]}
                        </button>
                      ))}
                    </div>
                  </div>
                  {prefsError && <p className="place__msg is-error" role="alert">{prefsError}</p>}
                </div>
              )}
            </>
          )}

          {step === 3 && (
            <section className="welcome__keys" aria-labelledby="welcome-title">
              <header className="welcome__head">
                <h1 id="welcome-title">Trois raccourcis à retenir</h1>
              </header>
              <dl>
                <div>
                  <dt>Chercher un jeu</dt>
                  <dd>Tape n'importe où, ou <kbd>Ctrl</kbd><kbd>K</kbd></dd>
                </div>
                <div>
                  <dt>Jouer au jeu choisi</dt>
                  <dd><kbd>Entrée</kbd>{pad && <> ou <kbd className="pad">A</kbd></>}</dd>
                </div>
                <div>
                  <dt>Ouvrir sa fiche</dt>
                  <dd><kbd>Espace</kbd>{pad && <> ou <kbd className="pad">X</kbd></>}</dd>
                </div>
              </dl>
              {pad && (
                <p className="welcome__pad">Manette branchée : la croix se déplace partout, <kbd>LB</kbd><kbd>RB</kbd> changent de section, <kbd className="pad">B</kbd> ferme.</p>
              )}
            </section>
          )}
        </div>

        {info.preview && step > 0 && step < 3 && <p className="welcome__preview">Aperçu : rien n'est enregistré ni déplacé.</p>}

        <div className="welcome__go">
          <button ref={next} type="button" className="btn-play" data-nav onClick={forward}>
            {last ? "C'est parti" : step === 1 ? "Continuer" : "Suivant"}
          </button>
          {step > 0 && (
            <button type="button" className="btn-ghost welcome__back" data-nav onClick={() => setStep(stepAfter(step, -1) ?? 0)}>Retour</button>
          )}
          <span className="welcome__hint"><kbd>Entrée</kbd>{pad && <kbd className="pad">A</kbd>}</span>
          {!last && (
            <button type="button" className="welcome__link welcome__skip" data-nav onClick={() => leave(onDone)}>Passer l'accueil</button>
          )}
        </div>
      </div>
    </div>,
    document.body,
  );
}
