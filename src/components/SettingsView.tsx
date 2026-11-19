// Écran Paramètres (Ctrl+, ou bas de la barre latérale). Réglages gardés et appliqués côté Rust
// (`settings.rs`) : la page n'envoie que des valeurs fermées, des booléens et le texte d'un raccourci.
import { useCallback, useEffect, useRef, useState } from "react";
import {
  checkUpdate,
  chooseSavesDir,
  onSettingsChanged,
  pauseQuickShortcut,
  PLATFORM_LABEL,
  setAutoSaves,
  setKeepInTray,
  setLaunchAtStartup,
  setQuickShortcut,
  setSourceEnabled,
  setTheme,
  settings as getSettings,
  showAllHidden,
  THEME_LABEL,
  THEMES,
  welcomeAgain,
  type Platform,
  type ScanResult,
  type SettingsInfo,
  type SourceReport,
  type UpdateInfo,
  type WelcomeInfo,
} from "../lib/api";
import { applyTheme } from "../lib/theme";
import { DiagnosticPanel } from "./DiagnosticPanel";

interface Props {
  scan: ScanResult | null;
  /** Bibliothèque relue après l'activation ou la désactivation d'un launcher. */
  onScan: (scan: ScanResult) => void;
  /** Jeux ré-affichés par « Ré-afficher tous les jeux masqués ». */
  onShown: (ids: string[]) => void;
  /** « Revoir l'écran de premier lancement ». */
  onWelcome: (info: WelcomeInfo) => void;
  /** Mise à jour déjà trouvée (barre du haut), et son installation. */
  update: UpdateInfo | null;
  onInstallUpdate: (info: UpdateInfo) => void;
}

const DEFAULT_SHORTCUT = "Alt+Space";
const MODIFIER_CODES = new Set(["ControlLeft", "ControlRight", "AltLeft", "AltRight", "ShiftLeft", "ShiftRight", "MetaLeft", "MetaRight", "OSLeft", "OSRight"]);

/** Modificateurs tenus, pour l'aperçu pendant la saisie. */
const heldLabel = (e: KeyboardEvent) =>
  [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Maj", e.metaKey && "Windows"].filter(Boolean).join("+");

/** Combinaison au format du plugin (`Ctrl+Shift+KeyL`), validée ensuite côté Rust. */
const comboText = (e: KeyboardEvent) =>
  [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super", e.code].filter(Boolean).join("+");

const plural = (n: number, one: string, many: string) => `${n.toLocaleString("fr-FR")} ${n > 1 ? many : one}`;

/** État d'un launcher, en une ligne. */
function sourceState(s: SourceReport): string {
  if (s.disabled) return s.count ? `Désactivé, ${plural(s.count, "jeu écarté", "jeux écartés")}` : "Désactivé";
  if (s.error) return `Lecture impossible : ${s.error}`;
  if (!s.installed) return "Pas sur ce PC";
  return s.count ? plural(s.count, "jeu", "jeux") : "Installé, aucun jeu";
}

/** Interrupteur : un bouton `switch`, navigable comme le reste (`data-nav`). */
function Switch({ label, detail, checked, disabled, onChange }: { label: string; detail?: string; checked: boolean; disabled?: boolean; onChange: (v: boolean) => void }) {
  return (
    <div className="set__row">
      <span className="set__text">
        <span>{label}</span>
        {detail && <small>{detail}</small>}
      </span>
      <button type="button" role="switch" aria-checked={checked} aria-label={label} className="switch" data-nav disabled={disabled} onClick={() => onChange(!checked)}>
        <span />
      </button>
    </div>
  );
}

export function SettingsView({ scan, onScan, onShown, onWelcome, update, onInstallUpdate }: Props) {
  const [info, setInfo] = useState<SettingsInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Saisie du raccourci : `null` hors saisie, sinon les modificateurs tenus.
  const [capture, setCapture] = useState<string | null>(null);
  const [shortcutMsg, setShortcutMsg] = useState<{ text: string; error: boolean } | null>(null);
  const [busySource, setBusySource] = useState<Platform | null>(null);
  const [confirmShow, setConfirmShow] = useState(false);
  const [updateMsg, setUpdateMsg] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [showDiag, setShowDiag] = useState(false);
  const captureBtn = useRef<HTMLButtonElement>(null);

  const fail = (e: unknown) => setError(String(e));
  const reload = useCallback(() => {
    getSettings().then(setInfo).catch(fail);
  }, []);

  useEffect(() => {
    reload();
    // Changements venus du menu de la zone de notification.
    const off = onSettingsChanged(setInfo);
    return () => void off.then((f) => f()).catch(() => {});
  }, [reload]);

  // Saisie d'un raccourci : toutes les touches s'arrêtent ici (phase de capture), le raccourci
  // actuel est coupé côté Rust pour arriver jusqu'à la page. Échap annule, un clic ailleurs aussi.
  const capturing = capture !== null;
  useEffect(() => {
    if (!capturing) return;
    let done = false;
    const finish = (text: string | null) => {
      if (done) return;
      done = true;
      setCapture(null);
      const save = text === null ? Promise.resolve() : setQuickShortcut(text).then((s) => {
        setInfo(s);
        setShortcutMsg({ text: s.quickShortcut ? `Recherche rapide : ${s.quickShortcutLabel}` : "Recherche rapide désactivée", error: false });
      });
      save
        .catch((e) => setShortcutMsg({ text: String(e), error: true }))
        .finally(() => {
          pauseQuickShortcut(false).catch(() => {});
          captureBtn.current?.focus();
        });
    };
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.type === "keyup") return void setCapture(heldLabel(e));
      if (e.key === "Escape" && !e.ctrlKey && !e.altKey && !e.metaKey) return finish(null);
      if (MODIFIER_CODES.has(e.code) || e.key === "Dead") return void setCapture(heldLabel(e));
      if (!e.ctrlKey && !e.altKey && !e.metaKey) {
        setShortcutMsg({ text: "Ajoute Ctrl, Alt ou Windows à la touche.", error: true });
        return;
      }
      finish(comboText(e));
    };
    const cancel = () => finish(null);
    pauseQuickShortcut(true).catch(() => {});
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("keyup", onKey, true);
    window.addEventListener("blur", cancel);
    window.addEventListener("pointerdown", cancel, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("keyup", onKey, true);
      window.removeEventListener("blur", cancel);
      window.removeEventListener("pointerdown", cancel, true);
      if (!done) pauseQuickShortcut(false).catch(() => {});
    };
  }, [capturing]);

  const changeShortcut = (text: string) => {
    setShortcutMsg(null);
    setQuickShortcut(text)
      .then((s) => {
        setInfo(s);
        setShortcutMsg({ text: s.quickShortcut ? `Recherche rapide : ${s.quickShortcutLabel}` : "Recherche rapide désactivée", error: false });
      })
      .catch((e) => setShortcutMsg({ text: String(e), error: true }));
  };

  const changeTheme = (theme: SettingsInfo["theme"]) => {
    applyTheme(theme); // tout de suite, sans attendre Rust
    setInfo((i) => i && { ...i, theme });
    setTheme(theme).then(setInfo).catch(fail);
  };

  const toggle = (patch: Partial<SettingsInfo>, run: () => Promise<SettingsInfo | void>) => {
    setInfo((i) => i && { ...i, ...patch });
    run()
      .then((s) => (s ? setInfo(s) : reload()))
      .catch((e) => {
        fail(e);
        reload();
      });
  };

  const chooseDir = () => {
    chooseSavesDir()
      .then((dir) => dir && setInfo((i) => i && { ...i, savesDir: dir }))
      .catch(fail);
  };

  const toggleSource = (platform: Platform, value: boolean) => {
    setBusySource(platform);
    setSourceEnabled(platform, value)
      .then((s) => {
        onScan(s);
        reload();
      })
      .catch(fail)
      .finally(() => setBusySource(null));
  };

  const unhide = () => {
    setConfirmShow(false);
    showAllHidden()
      .then((ids) => {
        onShown(ids);
        reload();
      })
      .catch(fail);
  };

  const searchUpdate = () => {
    setChecking(true);
    setUpdateMsg(null);
    checkUpdate()
      .then((u) => setUpdateMsg(u ? null : "Ludex est à jour."))
      .catch((e) => setUpdateMsg(String(e)))
      .finally(() => setChecking(false));
  };

  const sources = scan?.sources ?? [];

  return (
    <section className="set" aria-label="Paramètres">
      <div className="bar">
        <h2>Paramètres</h2>
      </div>
      {error && <p className="set__msg is-error" role="alert">{error}</p>}
      {!info ? (
        <p className="set__note">Lecture des réglages…</p>
      ) : (
        <>
          <section className="set__group" aria-labelledby="set-look">
            <h3 id="set-look">Apparence</h3>
            <div className="set__row set__row--stack">
              <span className="set__text">
                <span>Thème</span>
                <small>
                  « Comme Windows » suit le mode clair ou sombre de Windows. « Noir OLED » éteint le fond des écrans OLED, « Contraste élevé » renforce textes et contours.
                </small>
              </span>
              <div className="seg" role="radiogroup" aria-label="Thème">
                {THEMES.map((t) => (
                  <button key={t} type="button" role="radio" aria-checked={info.theme === t} aria-pressed={info.theme === t} data-nav onClick={() => changeTheme(t)}>
                    {THEME_LABEL[t]}
                  </button>
                ))}
              </div>
            </div>
          </section>

          <section className="set__group" aria-labelledby="set-quick">
            <h3 id="set-quick">Recherche rapide</h3>
            <div className="set__row">
              <span className="set__text">
                <span>Raccourci</span>
                <small>Ouvre la recherche par-dessus n'importe quelle application.</small>
              </span>
              <div className="set__acts">
                <button
                  ref={captureBtn}
                  type="button"
                  className={`set__key ${capturing ? "is-capturing" : ""}`}
                  data-nav
                  aria-label={capturing ? "Appuie sur la nouvelle combinaison, Échap pour annuler" : `Raccourci : ${info.quickShortcutLabel}. Changer`}
                  onClick={() => {
                    setShortcutMsg(null);
                    setCapture("");
                  }}
                >
                  {capturing ? (capture ? `${capture}+…` : "Appuie sur une combinaison") : info.quickShortcut ? <kbd>{info.quickShortcutLabel}</kbd> : "Désactivé"}
                </button>
                {info.quickShortcut && !capturing && (
                  <button type="button" className="btn-ghost set__small" data-nav onClick={() => changeShortcut("")}>Désactiver</button>
                )}
                {info.quickShortcut !== DEFAULT_SHORTCUT && !capturing && (
                  <button type="button" className="btn-ghost set__small" data-nav onClick={() => changeShortcut(DEFAULT_SHORTCUT)}>Alt+Espace</button>
                )}
              </div>
            </div>
            {capturing && <p className="set__note">Appuie sur la combinaison voulue (Ctrl, Alt ou Windows et une touche). Échap pour annuler.</p>}
            {shortcutMsg && <p className={`set__msg ${shortcutMsg.error ? "is-error" : ""}`} role="status">{shortcutMsg.text}</p>}
          </section>

          <section className="set__group" aria-labelledby="set-start">
            <h3 id="set-start">Démarrage et fermeture</h3>
            <Switch
              label="Lancer avec Windows"
              detail="Ludex démarre caché dans la zone de notification."
              checked={info.launchAtStartup}
              onChange={(v) => toggle({ launchAtStartup: v }, () => setLaunchAtStartup(v))}
            />
            <Switch
              label="Garder Ludex dans la zone de notification à la fermeture"
              detail="Le temps des jeux Epic, Ubisoft, Rockstar, Xbox et Riot continue d'être mesuré."
              checked={info.keepInTray}
              onChange={(v) => toggle({ keepInTray: v }, () => setKeepInTray(v))}
            />
          </section>

          <section className="set__group" aria-labelledby="set-saves">
            <h3 id="set-saves">Sauvegardes</h3>
            <div className="set__row">
              <span className="set__text">
                <span>Dossier des copies</span>
                <small className="path" title={info.savesDir}>{info.savesDir}</small>
              </span>
              <button type="button" className="btn-ghost set__small" data-nav onClick={chooseDir}>Changer le dossier</button>
            </div>
            <Switch
              label="Copier à la fin de chaque partie"
              detail="Parties lancées depuis Ludex, d'une minute au moins. 5 copies gardées par jeu."
              checked={info.autoSaves}
              onChange={(v) => toggle({ autoSaves: v }, () => setAutoSaves(v))}
            />
          </section>

          <section className="set__group" aria-labelledby="set-sources">
            <h3 id="set-sources">Launchers</h3>
            <p className="set__note">Un launcher désactivé reste lu, mais ses jeux n'apparaissent plus dans Ludex. Les jeux ajoutés à la main restent.</p>
            {sources.length === 0 && <p className="set__note">Bibliothèque en cours de lecture…</p>}
            {sources.map((s) => (
              <Switch
                key={s.platform}
                label={s.platform === "other" ? "Dossiers de jeux (Autres)" : PLATFORM_LABEL[s.platform]}
                detail={busySource === s.platform ? "Lecture de la bibliothèque…" : sourceState(s)}
                checked={!s.disabled}
                disabled={busySource !== null}
                onChange={(v) => toggleSource(s.platform, v)}
              />
            ))}
          </section>

          <section className="set__group" aria-labelledby="set-library">
            <h3 id="set-library">Bibliothèque</h3>
            <div className="set__row">
              <span className="set__text">
                <span>Jeux masqués</span>
                <small>
                  {info.hiddenGames ? `${plural(info.hiddenGames, "jeu masqué", "jeux masqués")} par toi.` : "Aucun jeu masqué par toi."} Les applis masquées
                  d'office (éditeurs, serveurs) restent dans le filtre Masqués.
                </small>
              </span>
              {confirmShow ? (
                <div className="set__acts">
                  <button type="button" className="btn-ghost set__small is-primary" data-nav autoFocus onClick={unhide}>
                    Ré-afficher {plural(info.hiddenGames, "jeu", "jeux")}
                  </button>
                  <button type="button" className="btn-ghost set__small" data-nav onClick={() => setConfirmShow(false)}>Annuler</button>
                </div>
              ) : (
                <button type="button" className="btn-ghost set__small" data-nav disabled={!info.hiddenGames} onClick={() => setConfirmShow(true)}>
                  Tout ré-afficher
                </button>
              )}
            </div>
            <div className="set__row">
              <span className="set__text">
                <span>Écran de premier lancement</span>
                <small>Ce que Ludex a trouvé et les raccourcis essentiels.</small>
              </span>
              <button type="button" className="btn-ghost set__small" data-nav onClick={() => welcomeAgain().then(onWelcome).catch(fail)}>Revoir</button>
            </div>
          </section>

          <section className="set__group" aria-labelledby="set-about">
            <h3 id="set-about">À propos</h3>
            <div className="set__row">
              <span className="set__text">
                <span>Ludex {info.version}</span>
                <small aria-live="polite">
                  {update ? `Ludex ${update.version} est disponible.` : checking ? "Recherche en cours…" : updateMsg ?? "Mises à jour vérifiées toutes les 12 heures."}
                </small>
              </span>
              {update ? (
                <button type="button" className="btn-ghost set__small is-primary" data-nav onClick={() => onInstallUpdate(update)}>Installer et relancer</button>
              ) : (
                <button type="button" className="btn-ghost set__small" data-nav disabled={checking} onClick={searchUpdate}>Rechercher une mise à jour</button>
              )}
            </div>
          </section>

          <section className="set__group" aria-labelledby="set-diag">
            <h3 id="set-diag">Diagnostic</h3>
            {showDiag ? (
              <DiagnosticPanel onScan={onScan} />
            ) : (
              <div className="set__row">
                <span className="set__text">
                  <span>Diagnostic des launchers</span>
                  <small>Ce que Ludex lit pour chaque launcher, les jeux écartés et les erreurs, avec un rapport à copier.</small>
                </span>
                <button type="button" className="btn-ghost set__small" data-nav onClick={() => setShowDiag(true)}>Afficher</button>
              </div>
            )}
          </section>
        </>
      )}
    </section>
  );
}
