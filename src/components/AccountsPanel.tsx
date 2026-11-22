import { useEffect, useState, type FormEvent } from "react";
import {
  epicAccount,
  epicLogin,
  epicLogout,
  gogAccount,
  gogLogin,
  gogLogout,
  onEpicAccount,
  onGogAccount,
  onSteamKey,
  steamKey,
  steamKeyPage,
  steamKeyRemove,
  steamKeySave,
  type GogAccount,
  type SteamKeyView,
  type Platform,
} from "../lib/api";
import { accountText } from "../lib/gog";
import { t } from "../lib/i18n";
import { epicAccountText, steamKeyText } from "../lib/notInstalled";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { LauncherIcon } from "./LauncherIcon";

/** Ce qui change d'un compte à l'autre (commandes et textes). */
interface AccountKind {
  key: string;
  platform: Platform;
  name: string;
  help: string;
  connect: string;
  showWindow: string;
  disconnectHint: string;
  signedIn: string;
  signedOut: string;
  text: (a: GogAccount) => string;
  get: () => Promise<GogAccount>;
  login: () => Promise<GogAccount>;
  logout: () => Promise<GogAccount>;
  listen: (handler: (a: GogAccount) => void) => Promise<UnlistenFn>;
}

function kinds(): AccountKind[] {
  const c = t.settings.accounts;
  return [
    {
      key: "gog",
      platform: "gog",
      name: c.gogName,
      help: c.gogHelp,
      connect: c.connect,
      showWindow: c.showWindow,
      disconnectHint: c.disconnectHint,
      signedIn: c.signedIn,
      signedOut: c.signedOut,
      text: accountText,
      get: gogAccount,
      login: gogLogin,
      logout: gogLogout,
      listen: onGogAccount,
    },
    {
      key: "epic",
      platform: "epic",
      name: c.epicName,
      help: c.epicHelp,
      connect: c.epicConnect,
      showWindow: c.epicShowWindow,
      disconnectHint: c.epicDisconnectHint,
      signedIn: c.epicSignedIn,
      signedOut: c.epicSignedOut,
      text: epicAccountText,
      get: epicAccount,
      login: epicLogin,
      logout: epicLogout,
      listen: onEpicAccount,
    },
  ];
}

/** Une ligne de compte : état, connexion dans la fenêtre de la boutique, déconnexion. */
function AccountRow({ kind, onToast }: { kind: AccountKind; onToast?: (title: string) => void }) {
  const [account, setAccount] = useState<GogAccount | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const c = t.settings.accounts;

  useEffect(() => {
    let alive = true;
    kind
      .get()
      .then((a) => alive && setAccount(a))
      .catch((e) => alive && setError(String(e)));
    let was: GogAccount["state"] | null = null;
    const off = kind.listen((a) => {
      setAccount(a);
      setError(a.error);
      if (was !== null && was !== "connected" && a.state === "connected") onToast?.(kind.signedIn);
      was = a.state;
    });
    return () => {
      alive = false;
      off.then((stop) => stop()).catch(() => {});
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const run = async (call: () => Promise<GogAccount>, done?: string) => {
    setBusy(true);
    setError(null);
    try {
      setAccount(await call());
      if (done) onToast?.(done);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const state = account?.state ?? "off";
  const help = `set-${kind.key}-help`;
  return (
    <>
      <div className="set__row">
        <span className="set__text">
          <span className="with-icon"><LauncherIcon platform={kind.platform} size={16} />{kind.name}</span>
          <small id={help}>
            {account ? kind.text(account) : "…"}. {kind.help}
          </small>
        </span>
        <div className="set__acts">
          {state === "connected" ? (
            <button type="button" className="btn-ghost set__small" data-nav disabled={busy} title={kind.disconnectHint} onClick={() => run(kind.logout, kind.signedOut)}>
              {c.disconnect}
            </button>
          ) : (
            <>
              <button type="button" className="btn-ghost set__small is-primary" data-nav disabled={busy} aria-describedby={help} onClick={() => run(kind.login)}>
                {account?.loginOpen ? kind.showWindow : state === "expired" ? c.reconnect : kind.connect}
              </button>
              {state === "expired" && (
                <button type="button" className="btn-ghost set__small" data-nav disabled={busy} title={kind.disconnectHint} onClick={() => run(kind.logout, kind.signedOut)}>
                  {c.disconnect}
                </button>
              )}
            </>
          )}
        </div>
      </div>
      {error && <p className="set__note gog__error" role="alert">{error}</p>}
    </>
  );
}

/**
 * Clé Web API Steam facultative : collée ici, vérifiée et chiffrée côté Rust, jamais renvoyée au
 * front (seulement son état). Sans clé, « Pas installés » garde la liste locale de Steam.
 */
function SteamKeyRow({ onToast }: { onToast?: (title: string) => void }) {
  const [view, setView] = useState<SteamKeyView | null>(null);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const c = t.settings.accounts;

  useEffect(() => {
    let alive = true;
    steamKey()
      .then((v) => alive && setView(v))
      .catch((e) => alive && setError(String(e)));
    const off = onSteamKey((v) => {
      setView(v);
      setError(v.error);
    });
    return () => {
      alive = false;
      off.then((stop) => stop()).catch(() => {});
    };
  }, []);

  const save = async (e: FormEvent) => {
    e.preventDefault();
    if (!draft.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      const lib = await steamKeySave(draft);
      setView(lib.key);
      setDraft("");
      onToast?.(c.steamSaved);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    setBusy(true);
    setError(null);
    try {
      const lib = await steamKeyRemove();
      setView(lib.key);
      onToast?.(c.steamRemoved);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const state = view?.state ?? "off";
  return (
    <>
      <div className="set__row">
        <span className="set__text">
          <span className="with-icon"><LauncherIcon platform="steam" size={16} />{c.steamName}</span>
          <small id="set-steam-help">
            {view ? steamKeyText(state) : "…"}. {c.steamHelp}
          </small>
        </span>
        <div className="set__acts">
          {state !== "off" && (
            <button type="button" className="btn-ghost set__small" data-nav disabled={busy} title={c.steamRemoveHint} onClick={remove}>
              {c.steamRemove}
            </button>
          )}
          <button type="button" className="btn-ghost set__small" data-nav title={c.steamGetKeyHint} onClick={() => steamKeyPage().catch((e) => setError(String(e)))}>
            {c.steamGetKey}
          </button>
        </div>
      </div>
      {state !== "connected" && (
        <form className="set__row steamkey" onSubmit={save}>
          <input
            type="password"
            className="inline-input steamkey__input"
            data-nav
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder={c.steamKeyPlaceholder}
            aria-label={c.steamKeyLabel}
            aria-describedby="set-steam-help"
            autoComplete="off"
            spellCheck={false}
            maxLength={64}
            disabled={busy}
          />
          <button type="submit" className="btn-ghost set__small is-primary" data-nav disabled={busy || !draft.trim()}>
            {busy ? c.steamSaving : c.steamSave}
          </button>
        </form>
      )}
      {error && <p className="set__note gog__error" role="alert">{error}</p>}
    </>
  );
}

/**
 * Section « Comptes » des Paramètres : comptes GOG et Epic Games facultatifs, puis clé Web API
 * Steam facultative (`SteamKeyRow`). « Se connecter »
 * ouvre la page de connexion de la boutique dans une fenêtre à part (créée par Rust, sans accès
 * à Ludex) ; aucun jeton ne vient jusqu'ici, seulement l'état et le pseudo.
 */
export function AccountsPanel({ onToast }: { onToast?: (title: string) => void }) {
  const c = t.settings.accounts;
  return (
    <section className="set__group" aria-labelledby="set-accounts">
      <h3 id="set-accounts">{c.title}</h3>
      {kinds().map((kind) => (
        <AccountRow key={kind.key} kind={kind} onToast={onToast} />
      ))}
      <SteamKeyRow onToast={onToast} />
    </section>
  );
}
