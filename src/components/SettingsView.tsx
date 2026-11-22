// Écran Paramètres (Ctrl+, ou bas de la barre latérale). Réglages gardés et appliqués côté Rust
// (`settings.rs`) : la page n'envoie que des valeurs fermées, des booléens et le texte d'un raccourci.
import { useCallback, useEffect, useId, useRef, useState, type ReactNode } from "react";
import { LauncherIcon } from "./LauncherIcon";
import { AccountsPanel } from "./AccountsPanel";
import {
  ACCENT_LABEL,
  ACCENTS,
  applyImport,
  cancelImport,
  checkUpdate,
  chooseSavesDir,
  exportData,
  importData,
  onSettingsChanged,
  openRepository,
  pauseQuickShortcut,
  PLATFORM_LABEL,
  setAccent,
  setAutoSaves,
  setDiscordHideName,
  setDiscordPresence,
  setKeepInTray,
  setLanguage,
  setLaunchAtStartup,
  setLaunchScreen,
  setNotification,
  setQuickShortcut,
  setSourceEnabled,
  setTheme,
  settings as getSettings,
  showAllHidden,
  THEME_LABEL,
  THEMES,
  welcomeAgain,
  type ImportDone,
  type ImportMode,
  type ImportPreview,
  type Platform,
  type ScanResult,
  type SettingsInfo,
  type SourceReport,
  type UpdateInfo,
  type WelcomeInfo,
} from "../lib/api";
import { applyAccent, applyTheme } from "../lib/theme";
import { CheckIcon } from "./Icons";
import { NOTIFICATION_ROWS } from "../lib/notifications";
import { discordHelp } from "../lib/discord";
import { DiagnosticPanel } from "./DiagnosticPanel";
import { DataPlace } from "./DataPlace";
import { previewOrigin, summaryText } from "../lib/transfer";
import { LANGUAGE_SETTINGS, t, type LanguageSetting } from "../lib/i18n";

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
  /** Données importées : bibliothèque relue (collections, statuts, jeux manuels…). */
  onImported: (done: ImportDone) => void;
  onToast: (toast: { title: string; detail?: string; error?: boolean }) => void;
}

const DEFAULT_SHORTCUT = "Alt+Space";
/** Services en ligne listés dans « À propos », dans cet ordre (textes dans `settings.about.services`). */
const ABOUT_SERVICES = ["updates", "steam", "store", "hltb", "saves", "images", "accounts"] as const;
const MODIFIER_CODES = new Set(["ControlLeft", "ControlRight", "AltLeft", "AltRight", "ShiftLeft", "ShiftRight", "MetaLeft", "MetaRight", "OSLeft", "OSRight"]);

/** Modificateurs tenus, pour l'aperçu pendant la saisie. */
const heldLabel = (e: KeyboardEvent) =>
  [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && t.settings.quick.shift, e.metaKey && "Windows"].filter(Boolean).join("+");

/** Combinaison au format du plugin (`Ctrl+Shift+KeyL`), validée ensuite côté Rust. */
const comboText = (e: KeyboardEvent) =>
  [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super", e.code].filter(Boolean).join("+");

/** État d'un launcher, en une ligne. */
function sourceState(s: SourceReport): string {
  const text = t.settings.sources;
  if (s.disabled) return s.count ? text.disabledCount(s.count) : text.disabled;
  if (s.error) return text.unreadable(s.error);
  if (!s.installed) return text.absent;
  return s.count ? text.games(s.count) : text.empty;
}

/** Interrupteur : un bouton `switch`, navigable comme le reste (`data-nav`). */
export function Switch({ label, icon, detail, checked, disabled, onChange }: { label: string; icon?: ReactNode; detail?: string; checked: boolean; disabled?: boolean; onChange: (v: boolean) => void }) {
  // Le texte d'aide sous le libellé est lu après le nom de l'interrupteur.
  const detailId = useId();
  return (
    <div className="set__row">
      <span className="set__text">
        <span className={icon ? "with-icon" : undefined}>{icon}{label}</span>
        {detail && <small id={detailId}>{detail}</small>}
      </span>
      <button type="button" role="switch" aria-checked={checked} aria-label={label} aria-describedby={detail ? detailId : undefined} className="switch" data-nav disabled={disabled} onClick={() => onChange(!checked)}>
        <span />
      </button>
    </div>
  );
}

export function SettingsView({ scan, onScan, onShown, onWelcome, update, onInstallUpdate, onImported, onToast }: Props) {
  const [info, setInfo] = useState<SettingsInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Saisie du raccourci : `null` hors saisie, sinon les modificateurs tenus.
  const [capture, setCapture] = useState<string | null>(null);
  const [shortcutMsg, setShortcutMsg] = useState<{ text: string; error: boolean } | null>(null);
  const [busySource, setBusySource] = useState<Platform | null>(null);
  const [confirmShow, setConfirmShow] = useState(false);
  const [updateMsg, setUpdateMsg] = useState<string | null>(null);
  const [repoError, setRepoError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [showDiag, setShowDiag] = useState(false);
  const captureBtn = useRef<HTMLButtonElement>(null);
  // Export et import : opération en cours, archive vérifiée qui attend sa confirmation.
  const [transfer, setTransfer] = useState<"export" | "import" | "apply" | null>(null);
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [confirmReplace, setConfirmReplace] = useState(false);
  const [transferMsg, setTransferMsg] = useState<{ text: string; error: boolean } | null>(null);
  const importBtn = useRef<HTMLButtonElement>(null);
  const pending = useRef(false);
  pending.current = preview !== null;
  // Page quittée avant de confirmer : l'archive gardée côté Rust est oubliée.
  useEffect(
    () => () => {
      if (pending.current) cancelImport().catch(() => {});
    },
    [],
  );

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
        setShortcutMsg({ text: s.quickShortcut ? t.settings.quick.saved(s.quickShortcutLabel) : t.settings.quick.disabled, error: false });
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
        setShortcutMsg({ text: t.settings.quick.needsModifier, error: true });
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
        setShortcutMsg({ text: s.quickShortcut ? t.settings.quick.saved(s.quickShortcutLabel) : t.settings.quick.disabled, error: false });
      })
      .catch((e) => setShortcutMsg({ text: String(e), error: true }));
  };

  const changeTheme = (theme: SettingsInfo["theme"]) => {
    applyTheme(theme); // tout de suite, sans attendre Rust
    setInfo((i) => i && { ...i, theme });
    setTheme(theme).then(setInfo).catch(fail);
  };

  const changeAccent = (accent: SettingsInfo["accent"]) => {
    applyAccent(accent); // tout de suite, sans attendre Rust
    setInfo((i) => i && { ...i, accent });
    setAccent(accent).then(setInfo).catch(fail);
  };

  // Langue : l'interface suit `settings-changed` (language.ts) ; le rescan renvoie la bibliothèque
  // relue dans la nouvelle langue (raisons d'écart, textes posés par Rust). Un seul changement à la fois.
  const languageBusy = useRef(false);
  const changeLanguage = (language: LanguageSetting) => {
    if (languageBusy.current || info?.language === language) return;
    languageBusy.current = true;
    setInfo((i) => i && { ...i, language });
    setLanguage(language)
      .then((s) => {
        onScan(s);
        reload();
      })
      .catch((e) => {
        fail(e);
        reload();
      })
      .finally(() => {
        languageBusy.current = false;
      });
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
      .then((u) => setUpdateMsg(u ? null : t.settings.about.upToDate))
      .catch((e) => setUpdateMsg(String(e)))
      .finally(() => setChecking(false));
  };

  const openRepo = () => {
    setRepoError(null);
    openRepository().catch((e) => setRepoError(String(e)));
  };

  const exportNow = () => {
    setTransfer("export");
    setTransferMsg(null);
    exportData()
      .then((info) => {
        if (!info) return;
        setTransferMsg({ text: t.settings.data.exported(info.file, summaryText(info.summary)), error: false });
        onToast({ title: t.settings.data.exportedToast, detail: info.file });
      })
      .catch((e) => setTransferMsg({ text: String(e), error: true }))
      .finally(() => setTransfer(null));
  };

  const chooseImport = () => {
    setTransfer("import");
    setTransferMsg(null);
    setConfirmReplace(false);
    importData()
      .then(setPreview)
      .catch((e) => {
        setPreview(null);
        setTransferMsg({ text: String(e), error: true });
      })
      .finally(() => setTransfer(null));
  };

  const closeImport = () => {
    setPreview(null);
    setConfirmReplace(false);
    // Le bouton « Importer » revient à la place des choix : le focus le suit.
    requestAnimationFrame(() => importBtn.current?.focus());
  };

  const dropImport = () => {
    cancelImport().catch(() => {});
    closeImport();
  };

  const runImport = (mode: ImportMode) => {
    setTransfer("apply");
    setTransferMsg(null);
    applyImport(mode)
      .then((done) => {
        onImported(done);
        reload();
        const data = t.settings.data;
        setTransferMsg({ text: data.imported(summaryText(done.summary), done.backup), error: false });
        onToast({ title: mode === "merge" ? data.mergedToast : data.replacedToast, detail: data.contents(summaryText(done.summary)) });
      })
      .catch((e) => {
        setTransferMsg({ text: String(e), error: true });
        cancelImport().catch(() => {});
      })
      .finally(() => {
        setTransfer(null);
        closeImport();
      });
  };

  const sources = scan?.sources ?? [];
  const tx = t.settings;

  return (
    <section className="set" aria-label={tx.title}>
      <div className="bar">
        <h2>{tx.title}</h2>
      </div>
      {error && <p className="set__msg is-error" role="alert">{error}</p>}
      {!info ? (
        <p className="set__note">{tx.loading}</p>
      ) : (
        <>
          <section className="set__group" aria-labelledby="set-look">
            <h3 id="set-look">{tx.look.title}</h3>
            <div className="set__row set__row--stack">
              <span className="set__text">
                <span>{tx.look.theme}</span>
                <small>{tx.look.themeHelp}</small>
              </span>
              <div className="seg" role="radiogroup" aria-label={tx.look.theme}>
                {THEMES.map((th) => (
                  <button key={th} type="button" role="radio" aria-checked={info.theme === th} data-nav onClick={() => changeTheme(th)}>
                    {THEME_LABEL[th]}
                  </button>
                ))}
              </div>
            </div>
            <div className="set__row">
              <span className="set__text">
                <span id="set-accent">{tx.look.accent}</span>
                <small>{tx.look.accentHelp}</small>
              </span>
              <div className="swatches" role="radiogroup" aria-labelledby="set-accent">
                {ACCENTS.map((a) => (
                  <button
                    key={a}
                    type="button"
                    className="swatch"
                    role="radio"
                    aria-checked={info.accent === a}
                    aria-label={ACCENT_LABEL[a]}
                    title={ACCENT_LABEL[a]}
                    data-accent={a}
                    data-nav
                    onClick={() => changeAccent(a)}
                  >
                    <span>{info.accent === a && <CheckIcon />}</span>
                  </button>
                ))}
              </div>
            </div>
            <div className="set__row set__row--stack">
              <span className="set__text">
                <span id="set-language">{tx.look.language}</span>
                <small id="set-language-help">{tx.look.languageHelp}</small>
              </span>
              <div className="seg" role="radiogroup" aria-labelledby="set-language" aria-describedby="set-language-help">
                {LANGUAGE_SETTINGS.map((l) => (
                  <button
                    key={l}
                    type="button"
                    role="radio"
                    aria-checked={info.language === l}
                    // Chaque langue est écrite dans sa langue : prononcée comme telle par les lecteurs d'écran.
                    lang={l === "system" ? undefined : l}
                    data-nav
                    onClick={() => changeLanguage(l)}
                  >
                    {t.common.language[l]}
                  </button>
                ))}
              </div>
            </div>
            <Switch
              label={tx.look.launchScreen}
              detail={tx.look.launchScreenHelp}
              checked={info.launchScreen}
              onChange={(v) => toggle({ launchScreen: v }, () => setLaunchScreen(v))}
            />
          </section>

          <section className="set__group" aria-labelledby="set-quick">
            <h3 id="set-quick">{tx.quick.title}</h3>
            <div className="set__row">
              <span className="set__text">
                <span>{tx.quick.shortcut}</span>
                <small>{tx.quick.shortcutHelp}</small>
              </span>
              <div className="set__acts">
                <button
                  ref={captureBtn}
                  type="button"
                  className={`set__key ${capturing ? "is-capturing" : ""}`}
                  data-nav
                  aria-label={capturing ? tx.quick.capturingLabel : tx.quick.changeLabel(info.quickShortcutLabel)}
                  onClick={() => {
                    setShortcutMsg(null);
                    setCapture("");
                  }}
                >
                  {capturing ? (capture ? `${capture}+…` : tx.quick.pressCombo) : info.quickShortcut ? <kbd>{info.quickShortcutLabel}</kbd> : tx.quick.off}
                </button>
                {info.quickShortcut && !capturing && (
                  <button type="button" className="btn-ghost set__small" data-nav onClick={() => changeShortcut("")}>{tx.quick.disable}</button>
                )}
                {info.quickShortcut !== DEFAULT_SHORTCUT && !capturing && (
                  <button type="button" className="btn-ghost set__small" data-nav onClick={() => changeShortcut(DEFAULT_SHORTCUT)}>{tx.quick.defaultKey}</button>
                )}
              </div>
            </div>
            {capturing && <p className="set__note">{tx.quick.capturingNote}</p>}
            {shortcutMsg && <p className={`set__msg ${shortcutMsg.error ? "is-error" : ""}`} role="status">{shortcutMsg.text}</p>}
          </section>

          <section className="set__group" aria-labelledby="set-start">
            <h3 id="set-start">{tx.startup.title}</h3>
            <Switch
              label={tx.startup.launchAtStartup}
              detail={tx.startup.launchAtStartupHelp}
              checked={info.launchAtStartup}
              onChange={(v) => toggle({ launchAtStartup: v }, () => setLaunchAtStartup(v))}
            />
            <Switch
              label={tx.startup.keepInTray}
              detail={tx.startup.keepInTrayHelp}
              checked={info.keepInTray}
              onChange={(v) => toggle({ keepInTray: v }, () => setKeepInTray(v))}
            />
          </section>

          <section className="set__group" aria-labelledby="set-saves">
            <h3 id="set-saves">{tx.saves.title}</h3>
            <div className="set__row">
              <span className="set__text">
                <span>{tx.saves.folder}</span>
                <small className="path" title={info.savesDir}>{info.savesDir}</small>
              </span>
              <button type="button" className="btn-ghost set__small" data-nav onClick={chooseDir}>{tx.saves.changeFolder}</button>
            </div>
            <Switch
              label={tx.saves.auto}
              detail={tx.saves.autoHelp}
              checked={info.autoSaves}
              onChange={(v) => toggle({ autoSaves: v }, () => setAutoSaves(v))}
            />
          </section>

          <section className="set__group" aria-labelledby="set-place">
            <h3 id="set-place">{tx.storage.title}</h3>
            <p className="set__note">{tx.storage.note}</p>
            <DataPlace
              explainKept
              onMoved={(done) => {
                onScan(done.scan);
                reload();
                onToast({ title: t.settings.storage.movedToast, detail: done.location.root || t.settings.storage.original });
              }}
            />
          </section>

          <section className="set__group" aria-labelledby="set-notes">
            <h3 id="set-notes">{tx.notifications.title}</h3>
            <p className="set__note">{tx.notifications.note}</p>
            {info.notificationsBlocked && (
              <p className="set__msg" role="status">{tx.notifications.blocked}</p>
            )}
            {NOTIFICATION_ROWS.map((row) => (
              <Switch
                key={row.kind}
                label={row.label}
                detail={row.detail}
                checked={info.notifications[row.kind]}
                onChange={(v) => toggle({ notifications: { ...info.notifications, [row.kind]: v } }, () => setNotification(row.kind, v))}
              />
            ))}
          </section>

          <section className="set__group" aria-labelledby="set-discord">
            <h3 id="set-discord">{tx.discord.title}</h3>
            <Switch
              label={tx.discord.show}
              detail={discordHelp(info.discordAvailable, info.discordHideName)}
              checked={info.discordAvailable && info.discordPresence}
              disabled={!info.discordAvailable}
              onChange={(v) => toggle({ discordPresence: v }, () => setDiscordPresence(v))}
            />
            {info.discordAvailable && info.discordPresence && (
              <Switch
                label={tx.discord.hideName}
                detail={tx.discord.hideNameDetail}
                checked={info.discordHideName}
                onChange={(v) => toggle({ discordHideName: v }, () => setDiscordHideName(v))}
              />
            )}
          </section>

          <AccountsPanel onToast={(title) => onToast({ title })} />

          <section className="set__group" aria-labelledby="set-sources">
            <h3 id="set-sources">{tx.sources.title}</h3>
            <p className="set__note">{tx.sources.note}</p>
            {sources.length === 0 && <p className="set__note">{tx.sources.reading}</p>}
            {sources.map((s) => (
              <Switch
                key={s.platform}
                label={s.platform === "other" ? tx.sources.other : PLATFORM_LABEL[s.platform]}
                icon={<LauncherIcon platform={s.platform} size={16} />}
                detail={busySource === s.platform ? tx.sources.readingLibrary : sourceState(s)}
                checked={!s.disabled}
                disabled={busySource !== null}
                onChange={(v) => toggleSource(s.platform, v)}
              />
            ))}
          </section>

          <section className="set__group" aria-labelledby="set-library">
            <h3 id="set-library">{tx.library.title}</h3>
            <div className="set__row">
              <span className="set__text">
                <span>{tx.library.hidden}</span>
                <small>
                  {info.hiddenGames ? tx.library.hiddenByYou(info.hiddenGames) : tx.library.noneHidden} {tx.library.hiddenNote}
                </small>
              </span>
              {confirmShow ? (
                <div className="set__acts">
                  <button type="button" className="btn-ghost set__small is-primary" data-nav autoFocus onClick={unhide}>
                    {tx.library.unhide(info.hiddenGames)}
                  </button>
                  <button type="button" className="btn-ghost set__small" data-nav onClick={() => setConfirmShow(false)}>{tx.cancel}</button>
                </div>
              ) : (
                <button type="button" className="btn-ghost set__small" data-nav disabled={!info.hiddenGames} onClick={() => setConfirmShow(true)}>
                  {tx.library.unhideAll}
                </button>
              )}
            </div>
            <div className="set__row">
              <span className="set__text">
                <span>{tx.library.welcome}</span>
                <small>{tx.library.welcomeHelp}</small>
              </span>
              <button type="button" className="btn-ghost set__small" data-nav onClick={() => welcomeAgain().then(onWelcome).catch(fail)}>{tx.library.again}</button>
            </div>
          </section>

          <section className="set__group" aria-labelledby="set-data">
            <h3 id="set-data">{tx.data.title}</h3>
            <div className="set__row">
              <span className="set__text">
                <span>{tx.data.export}</span>
                <small>{tx.data.exportHelp}</small>
              </span>
              <button type="button" className="btn-ghost set__small" data-nav disabled={transfer !== null || preview !== null} onClick={exportNow}>
                {transfer === "export" ? tx.data.exporting : tx.data.exportBtn}
              </button>
            </div>
            <div className="set__row">
              <span className="set__text">
                <span>{tx.data.import}</span>
                <small>
                  {preview ? tx.data.preview(previewOrigin(preview), preview.file, summaryText(preview.summary)) : tx.data.importHelp}
                </small>
              </span>
              {!preview ? (
                <button ref={importBtn} type="button" className="btn-ghost set__small" data-nav disabled={transfer !== null} onClick={chooseImport}>
                  {transfer === "import" ? tx.data.reading : tx.data.importBtn}
                </button>
              ) : confirmReplace ? (
                <div className="set__acts">
                  <button type="button" className="btn-ghost btn-ghost--danger set__small" data-nav autoFocus disabled={transfer !== null} onClick={() => runImport("replace")}>
                    {transfer === "apply" ? tx.data.importing : tx.data.replaceAll}
                  </button>
                  <button type="button" className="btn-ghost set__small" data-nav disabled={transfer !== null} onClick={() => setConfirmReplace(false)}>{tx.data.back}</button>
                </div>
              ) : (
                <div className="set__acts">
                  <button type="button" className="btn-ghost set__small is-primary" data-nav autoFocus disabled={transfer !== null} onClick={() => runImport("merge")}>
                    {transfer === "apply" ? tx.data.importing : tx.data.merge}
                  </button>
                  <button type="button" className="btn-ghost set__small" data-nav disabled={transfer !== null} onClick={() => setConfirmReplace(true)}>{tx.data.replace}</button>
                  <button type="button" className="btn-ghost set__small" data-nav disabled={transfer !== null} onClick={dropImport}>{tx.cancel}</button>
                </div>
              )}
            </div>
            {preview && <p className="set__note">{confirmReplace ? tx.data.replaceNote : tx.data.mergeNote}</p>}
            {transferMsg && <p className={`set__msg ${transferMsg.error ? "is-error" : ""}`} role="status">{transferMsg.text}</p>}
          </section>

          <section className="set__group" aria-labelledby="set-about">
            <h3 id="set-about">{tx.about.title}</h3>
            <div className="set__row">
              <span className="set__text">
                <span>Ludex {info.version}</span>
                <small aria-live="polite">
                  {update ? tx.about.available(update.version) : checking ? tx.about.checking : updateMsg ?? tx.about.schedule}
                </small>
              </span>
              {update ? (
                <button type="button" className="btn-ghost set__small is-primary" data-nav onClick={() => onInstallUpdate(update)}>{tx.about.install}</button>
              ) : (
                <button type="button" className="btn-ghost set__small" data-nav disabled={checking} onClick={searchUpdate}>{tx.about.check}</button>
              )}
            </div>
            <div className="set__row">
              <span className="set__text">
                <span>{tx.about.repo}</span>
                <small>{tx.about.repoHelp}</small>
              </span>
              <button type="button" className="btn-ghost set__small" data-nav onClick={openRepo}>{tx.about.openRepo}</button>
            </div>
            {repoError && <p className="set__msg is-error" role="alert">{repoError}</p>}
            <h4 className="set__sub" id="set-services">{tx.about.servicesTitle}</h4>
            <ul className="set__services" aria-labelledby="set-services">
              {ABOUT_SERVICES.map((key) => (
                <li key={key}>
                  <span>{tx.about.services[key].name}</span>
                  <small>{tx.about.services[key].sent}</small>
                </li>
              ))}
            </ul>
            <p className="set__note">{tx.about.servicesNote}</p>
            <p className="set__note">{tx.about.notAffiliated}</p>
          </section>

          <section className="set__group" aria-labelledby="set-diag">
            <h3 id="set-diag">{tx.diagSection.title}</h3>
            {showDiag ? (
              <DiagnosticPanel scan={scan} onScan={onScan} />
            ) : (
              <div className="set__row">
                <span className="set__text">
                  <span>{tx.diagSection.label}</span>
                  <small>{tx.diagSection.help}</small>
                </span>
                <button type="button" className="btn-ghost set__small" data-nav onClick={() => setShowDiag(true)}>{tx.diagSection.show}</button>
              </div>
            )}
          </section>
        </>
      )}
    </section>
  );
}
