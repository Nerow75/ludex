// Thèmes perso (Paramètres > Apparence) : liste des thèmes gardés, éditeur avec aperçu en direct et
// contrôle des contrastes, partage en texte court. Calculs dans `src/lib/customTheme.ts`,
// validation et stockage côté Rust (`custom_theme.rs`).
import { useId, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { deleteCustomTheme, saveCustomTheme, setCustomTheme, type SettingsInfo } from "../lib/api";
import {
  MAX_THEMES,
  BASE_COLORS,
  COLOR_KEYS,
  THEME_BASES,
  cleanHex,
  cleanThemeName,
  deriveTheme,
  MAX_SHARE,
  MAX_THEME_NAME,
  parseShareCode,
  shareCode,
  type CustomTheme,
  type DerivedTheme,
  type ThemeBase,
  type ThemeColors,
} from "../lib/customTheme";
import { formatNumber, t } from "../lib/i18n";
import { activeCustomTheme, applyCustomTheme } from "../lib/theme";
import { markFocus, restoreFocus, type FocusMark } from "./focus";
import { CheckIcon, PencilIcon, PlusIcon, TrashIcon } from "./Icons";

interface Props {
  info: SettingsInfo;
  setInfo: (info: SettingsInfo) => void;
  onToast: (toast: { title: string; detail?: string; error?: boolean }) => void;
}

type Draft = ThemeColors & { id: string | null; name: string; base: ThemeBase };

/** Variables d'un thème calculé, posées sur l'aperçu (elles s'héritent comme sur `<html>`). */
const previewStyle = (d: DerivedTheme): CSSProperties => ({ ...(d.vars as CSSProperties), colorScheme: d.scheme });

/** Les flèches et Entrée d'un champ restent au champ (pas de navigation de la page). */
const keepKeys = (e: ReactKeyboardEvent) => {
  if (e.key !== "Escape" && e.key !== "Tab") e.stopPropagation();
};

const ratio = (r: number) => `${formatNumber(Math.floor(r * 10) / 10, { minimumFractionDigits: 1, maximumFractionDigits: 1 })}:1`;

export function CustomThemes({ info, setInfo, onToast }: Props) {
  const tx = t.settings.themes;
  const active = activeCustomTheme(info);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [paste, setPaste] = useState<string | null>(null);
  const [shared, setShared] = useState<string | null>(null);
  const opener = useRef<FocusMark>(null);
  const listId = useId();

  const derived = useMemo(() => (draft ? deriveTheme(draft.base, draft) : null), [draft]);

  const choose = (theme: CustomTheme | null) => {
    setError(null);
    applyCustomTheme(theme); // tout de suite, sans attendre Rust
    setCustomTheme(theme?.id ?? null)
      .then(setInfo)
      .catch((e) => {
        setError(String(e));
        applyCustomTheme(active);
      });
  };

  const open = (next: Draft) => {
    opener.current = markFocus();
    setError(null);
    setPaste(null);
    setShared(null);
    setConfirmDelete(false);
    setDraft(next);
  };

  const close = () => {
    setDraft(null);
    setError(null);
    restoreFocus(opener.current);
  };

  // Nouveau thème : part du thème de base choisi (« Comme Windows » = sombre).
  const create = () => {
    const base = (THEME_BASES as readonly string[]).includes(info.theme) ? (info.theme as ThemeBase) : "dark";
    open({ id: null, name: "", base, ...BASE_COLORS[base] });
  };

  const save = () => {
    if (!draft || !derived) return;
    const name = cleanThemeName(draft.name);
    if (!name) return setError(tx.nameMissing);
    if (!derived.ok) return setError(tx.unreadable);
    saveCustomTheme({ id: draft.id, name, base: draft.base, bg: draft.bg, surface: draft.surface, text: draft.text, accent: draft.accent })
      .then((s) => {
        setInfo(s);
        applyCustomTheme(activeCustomTheme(s));
        onToast({ title: draft.id ? tx.savedToast(name) : tx.createdToast(name) });
        setDraft(null);
        setError(null);
        // Focus sur le thème enregistré (sa pastille vient d'être créée ou cochée).
        window.setTimeout(() => document.querySelector<HTMLElement>(`[data-theme-id="${CSS.escape(s.customTheme ?? "")}"]`)?.focus(), 0);
      })
      .catch((e) => setError(String(e)));
  };

  const remove = () => {
    if (!active) return;
    const name = active.name;
    deleteCustomTheme(active.id)
      .then((s) => {
        setInfo(s);
        applyCustomTheme(activeCustomTheme(s));
        setConfirmDelete(false);
        onToast({ title: tx.deletedToast(name) });
        window.setTimeout(() => document.querySelector<HTMLElement>(".tlist__new")?.focus(), 0);
      })
      .catch((e) => setError(String(e)));
  };

  const share = () => {
    if (!active) return;
    const code = shareCode(active);
    navigator.clipboard
      .writeText(code)
      .then(() => onToast({ title: tx.copiedToast, detail: code }))
      .catch(() => setShared(code)); // refusé : texte affiché, à sélectionner
  };

  const readPaste = () => {
    const theme = parseShareCode(paste ?? "");
    if (!theme) return setError(tx.pasteInvalid);
    open({ id: null, ...theme });
  };

  const setBase = (base: ThemeBase) => draft && setDraft({ ...draft, base, ...BASE_COLORS[base] });
  const setColor = (key: keyof ThemeColors, hex: string) => draft && setDraft({ ...draft, [key]: hex });

  return (
    <div className="set__row set__row--stack">
      <span className="set__text">
        <span id={listId}>{tx.title}</span>
        <small>{tx.help}</small>
      </span>

      {info.customThemes.length > 0 && (
        <div className="tlist" role="radiogroup" aria-labelledby={listId}>
          {info.customThemes.map((theme) => (
            <button
              key={theme.id}
              type="button"
              role="radio"
              className="tlist__item"
              aria-checked={active?.id === theme.id}
              aria-label={tx.itemLabel(theme.name, t.common.theme[theme.base])}
              data-theme-id={theme.id}
              data-nav
              onClick={() => choose(active?.id === theme.id ? null : theme)}
            >
              <span className="tlist__dots" aria-hidden>
                {COLOR_KEYS.map((k) => (
                  <i key={k} style={{ background: theme[k] }} />
                ))}
              </span>
              <span className="tlist__name">{theme.name}</span>
              {active?.id === theme.id && <CheckIcon />}
            </button>
          ))}
        </div>
      )}

      <div className="set__acts tlist__acts">
        {!draft && (
          <button type="button" className="btn-ghost set__small tlist__new" data-nav onClick={create} disabled={info.customThemes.length >= MAX_THEMES}>
            <PlusIcon />
            {tx.create}
          </button>
        )}
        {active && !draft && (
          <>
            <button type="button" className="btn-ghost set__small" data-nav onClick={() => open({ ...active })}>
              <PencilIcon />
              {tx.edit}
            </button>
            <button type="button" className="btn-ghost set__small" data-nav onClick={share}>
              {tx.share}
            </button>
            {confirmDelete ? (
              <>
                <button type="button" className="btn-ghost btn-ghost--danger set__small" data-nav onClick={remove} autoFocus>
                  {tx.confirmDelete(active.name)}
                </button>
                <button type="button" className="btn-ghost set__small" data-nav onClick={() => setConfirmDelete(false)}>
                  {t.settings.cancel}
                </button>
              </>
            ) : (
              <button type="button" className="btn-ghost set__small" data-nav onClick={() => setConfirmDelete(true)}>
                <TrashIcon />
                {tx.delete}
              </button>
            )}
          </>
        )}
        {!draft && paste === null && (
          <button type="button" className="btn-ghost set__small" data-nav onClick={() => (setPaste(""), setError(null))}>
            {tx.paste}
          </button>
        )}
      </div>

      {shared && (
        <label className="tedit__field">
          <span>{tx.copyByHand}</span>
          <input className="inline-input tedit__code" readOnly value={shared} onFocus={(e) => e.currentTarget.select()} onKeyDown={keepKeys} data-nav />
        </label>
      )}

      {paste !== null && !draft && (
        <div className="tedit__paste">
          <label className="tedit__field">
            <span>{tx.pasteLabel}</span>
            <input
              className="inline-input tedit__code"
              value={paste}
              maxLength={MAX_SHARE}
              spellCheck={false}
              placeholder="LUDEX:1:dark:0B0B0B:1A1A1A:FFFFFF:14B8A6:…"
              autoFocus
              data-nav
              onChange={(e) => setPaste(e.target.value)}
              onKeyDown={(e) => {
                keepKeys(e);
                if (e.key === "Enter") readPaste();
                if (e.key === "Escape") {
                  e.stopPropagation();
                  setPaste(null);
                  setError(null);
                }
              }}
            />
          </label>
          <div className="set__acts">
            <button type="button" className="btn-ghost set__small is-primary" data-nav onClick={readPaste}>{tx.pasteRead}</button>
            <button type="button" className="btn-ghost set__small" data-nav onClick={() => (setPaste(null), setError(null))}>{t.settings.cancel}</button>
          </div>
        </div>
      )}

      {draft && derived && (
        <div
          className="tedit"
          role="group"
          aria-label={draft.id ? tx.editing : tx.creating}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              e.stopPropagation();
              close();
            }
          }}
        >
          <div className="tedit__form">
            <label className="tedit__field">
              <span>{tx.name}</span>
              <input
                className="inline-input"
                value={draft.name}
                maxLength={MAX_THEME_NAME + 20}
                placeholder={tx.namePlaceholder}
                autoFocus
                data-nav
                onChange={(e) => setDraft({ ...draft, name: e.target.value })}
                onKeyDown={(e) => {
                  keepKeys(e);
                  if (e.key === "Enter") save();
                }}
              />
            </label>
            <div className="tedit__field">
              <span id={`${listId}-base`}>{tx.base}</span>
              <div className="seg" role="radiogroup" aria-labelledby={`${listId}-base`} aria-describedby={`${listId}-base-help`}>
                {THEME_BASES.map((b) => (
                  <button key={b} type="button" role="radio" aria-checked={draft.base === b} data-nav onClick={() => setBase(b)}>
                    {t.common.theme[b]}
                  </button>
                ))}
              </div>
              <small id={`${listId}-base-help`}>{tx.baseHelp}</small>
            </div>
            <div className="tedit__colors">
              {COLOR_KEYS.map((k) => (
                <ColorField key={k} label={tx.colors[k]} value={draft[k]} onChange={(hex) => setColor(k, hex)} />
              ))}
            </div>
          </div>

          <div className="tedit__side">
            <Preview derived={derived} name={cleanThemeName(draft.name) || tx.namePlaceholder} />
            <Checks derived={derived} />
          </div>

          {error && <p className="set__msg is-error" role="alert">{error}</p>}
          <div className="set__acts tedit__acts">
            <button type="button" className="btn-ghost set__small is-primary" data-nav onClick={save} disabled={!derived.ok}>
              {tx.save}
            </button>
            <button type="button" className="btn-ghost set__small" data-nav onClick={close}>
              {t.settings.cancel}
            </button>
          </div>
        </div>
      )}
      {!draft && error && <p className="set__msg is-error" role="alert">{error}</p>}
    </div>
  );
}

/** Couleur : sélecteur natif et code hexadécimal, l'un suit l'autre. */
function ColorField({ label, value, onChange }: { label: string; value: string; onChange: (hex: string) => void }) {
  const id = useId();
  const [text, setText] = useState<string | null>(null); // saisie en cours (sinon la valeur)
  const shown = text ?? value;
  const invalid = text !== null && !cleanHex(text);
  return (
    <div className="tedit__color">
      <label htmlFor={id}>{label}</label>
      <div className="tedit__pick">
        <input
          type="color"
          value={value.toLowerCase()}
          aria-label={t.settings.themes.picker(label)}
          data-nav
          onChange={(e) => {
            const hex = cleanHex(e.target.value);
            if (hex) {
              setText(null);
              onChange(hex);
            }
          }}
        />
        <input
          id={id}
          className="inline-input tedit__hex"
          value={shown}
          maxLength={7}
          spellCheck={false}
          aria-invalid={invalid}
          data-nav
          onChange={(e) => {
            setText(e.target.value);
            const hex = cleanHex(e.target.value);
            if (hex) onChange(hex);
          }}
          onBlur={() => setText(null)}
          onKeyDown={keepKeys}
        />
      </div>
    </div>
  );
}

/** Aperçu en direct : les vrais tokens calculés, posés sur un petit morceau d'interface. */
function Preview({ derived, name }: { derived: DerivedTheme; name: string }) {
  const tx = t.settings.themes.preview;
  return (
    <div className="tprev" style={previewStyle(derived)} aria-label={tx.label} role="img">
      <div className="tprev__side">
        <i className="is-on" />
        <i />
        <i />
      </div>
      <div className="tprev__main">
        <b>{name}</b>
        <span>{tx.secondary}</span>
        <small>{tx.muted}</small>
        <div className="tprev__card">
          <span className="tprev__play">{tx.play}</span>
          <span className="tprev__chip">{tx.chip}</span>
          <span className="tprev__switch" />
        </div>
        <div className="tprev__row">
          <span className="tprev__focus">{tx.focus}</span>
          <span className="tprev__accent">{tx.accentText}</span>
        </div>
      </div>
    </div>
  );
}

/** Contrastes vérifiés : tel quel, corrigé (avec la couleur retenue) ou impossible. */
function Checks({ derived }: { derived: DerivedTheme }) {
  const tx = t.settings.themes.checks;
  return (
    <ul className="tchecks" aria-label={tx.title}>
      {derived.checks.map((c) => (
        <li key={c.key} className={`is-${c.status}`}>
          <span className="tchecks__dot" aria-hidden />
          <span className="tchecks__text">
            <span>{tx.names[c.key]}</span>
            <small>
              {tx.status[c.status](ratio(c.ratio), ratio(c.target))}
              {c.from && c.to && (
                <>
                  {" "}
                  <i className="tchecks__sw" style={{ background: c.from }} aria-hidden />
                  {c.from} → <i className="tchecks__sw" style={{ background: c.to }} aria-hidden />
                  {c.to}
                </>
              )}
            </small>
          </span>
        </li>
      ))}
    </ul>
  );
}
