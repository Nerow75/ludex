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
import { formatNumber, t, useLang } from "../lib/i18n";
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

/** Réglages de l'étape 3 (lus côté Rust, simulés dans l'aperçu). */
interface Prefs {
  launchAtStartup: boolean;
  keepInTray: boolean;
  theme: Theme;
}

export default function WelcomeScreen({ scan, info, pad, onDone, onShowHidden, onScan }: Props) {
  // Résumé refait au changement de langue (phrase du titre, noms des sources).
  const language = useLang();
  const s = useMemo(() => summarize(scan), [scan, language]);
  const w = t.settings.welcome;
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
    s.empty.length > 0 && w.empty(s.empty.join(", ")),
    s.missing.length > 0 && w.missing(s.missing.join(", ")),
    s.failed.length > 0 && w.failed(s.failed.join(", ")),
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
          <ol className="welcome__steps" aria-label={w.stepsLabel}>
            {STEPS.map((label, i) => (
              <li key={label} className={i === step ? "is-current" : i < step ? "is-done" : ""} aria-current={i === step ? "step" : undefined}>
                <span className="sr-only">{label}</span>
              </li>
            ))}
          </ol>
        </div>

        {/* Changement d'étape annoncé (le focus passe au bouton principal, qui ne dit pas où l'on est). */}
        <p className="sr-only" aria-live="polite">{positionText(w.step, step, STEPS.length)}{w.stepSep}{STEPS[step]}</p>
        <div className="welcome__step" key={step}>
          {step === 0 && (
            <>
              <header className="welcome__head">
                <h1 id="welcome-title">
                  {s.total ? <><b>{w.games(s.total)}</b> {s.where}</> : w.none}
                </h1>
                {s.total === 0 && (
                  <p className="welcome__lead">{w.noneLead}</p>
                )}
              </header>

              {s.found.length > 0 && (
                <ul className="welcome__sources" aria-label={w.bySource}>
                  {s.found.map((f) => (
                    <li key={f.platform}>{f.label}<span>{formatNumber(f.count)}</span></li>
                  ))}
                </ul>
              )}

              <div className="welcome__facts">
                {others.map((line) => <p key={line}>{line}</p>)}
                {s.tools > 0 && (
                  <p>
                    {w.tools(s.tools)}{" "}
                    <button type="button" className="welcome__link" data-nav onClick={() => leave(onShowHidden)}>
                      {w.seeThem(s.tools)}
                    </button>
                  </p>
                )}
                {info.folders.length > 0 ? (
                  <p>
                    {w.folders(info.folders.length)}{" "}
                    {info.folders.map((f, i) => <span key={f}>{i > 0 && ", "}<code>{f}</code></span>)}
                  </p>
                ) : (
                  <p>{w.noFolders}<kbd>Ctrl</kbd><kbd>N</kbd>{w.noFoldersEnd}</p>
                )}
              </div>
            </>
          )}

          {step === 1 && (
            <>
              <header className="welcome__head">
                <h1 id="welcome-title">{w.dataTitle}</h1>
                <p className="welcome__lead">{w.dataLead}</p>
              </header>
              <DataPlace explainKept onMoved={(done: MoveDone) => onScan(done.scan)} />
            </>
          )}

          {step === 2 && (
            <>
              <header className="welcome__head">
                <h1 id="welcome-title">{w.prefsTitle}</h1>
                <p className="welcome__lead">{w.prefsLead}<kbd>Ctrl</kbd><kbd>,</kbd>{w.prefsLeadEnd}</p>
              </header>
              {!prefs ? (
                <p className="place__note">{prefsError ?? t.settings.loading}</p>
              ) : (
                <div className="welcome__prefs">
                  <Switch
                    label={t.settings.startup.launchAtStartup}
                    detail={t.settings.startup.launchAtStartupHelp}
                    checked={prefs.launchAtStartup}
                    onChange={(v) => change({ launchAtStartup: v }, () => setLaunchAtStartup(v))}
                  />
                  <Switch
                    label={t.settings.startup.keepInTray}
                    detail={t.settings.startup.keepInTrayHelp}
                    checked={prefs.keepInTray}
                    onChange={(v) => change({ keepInTray: v }, () => setKeepInTray(v))}
                  />
                  <div className="set__row set__row--stack">
                    <span className="set__text">
                      <span>{t.settings.look.theme}</span>
                      <small>{w.themeHelp}</small>
                    </span>
                    <div className="seg" role="radiogroup" aria-label={t.settings.look.theme}>
                      {THEMES.map((th) => (
                        <button key={th} type="button" role="radio" aria-checked={prefs.theme === th} data-nav onClick={() => changeTheme(th)}>
                          {THEME_LABEL[th]}
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
                <h1 id="welcome-title">{w.keysTitle}</h1>
              </header>
              <dl>
                <div>
                  <dt>{w.search}</dt>
                  <dd>{w.searchHow}<kbd>Ctrl</kbd><kbd>K</kbd></dd>
                </div>
                <div>
                  <dt>{w.play}</dt>
                  <dd><kbd>{w.enter}</kbd>{pad && <>{w.or}<kbd className="pad">A</kbd></>}</dd>
                </div>
                <div>
                  <dt>{w.details}</dt>
                  <dd><kbd>{w.space}</kbd>{pad && <>{w.or}<kbd className="pad">X</kbd></>}</dd>
                </div>
              </dl>
              {pad && (
                <p className="welcome__pad">{w.padBefore}<kbd>LB</kbd><kbd>RB</kbd>{w.padMiddle}<kbd className="pad">B</kbd>{w.padEnd}</p>
              )}
            </section>
          )}
        </div>

        {info.preview && step > 0 && step < 3 && <p className="welcome__preview">{w.preview}</p>}

        <div className="welcome__go">
          <button ref={next} type="button" className="btn-play" data-nav onClick={forward}>
            {last ? w.go : step === 1 ? w.continue : w.next}
          </button>
          {step > 0 && (
            <button type="button" className="btn-ghost welcome__back" data-nav onClick={() => setStep(stepAfter(step, -1) ?? 0)}>{w.back}</button>
          )}
          <span className="welcome__hint"><kbd>{w.enter}</kbd>{pad && <kbd className="pad">A</kbd>}</span>
          {!last && (
            <button type="button" className="welcome__link welcome__skip" data-nav onClick={() => leave(onDone)}>{w.skip}</button>
          )}
        </div>
      </div>
    </div>,
    document.body,
  );
}
