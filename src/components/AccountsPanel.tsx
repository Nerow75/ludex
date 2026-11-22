import { useEffect, useState } from "react";
import { gogAccount, gogLogin, gogLogout, onGogAccount, type GogAccount } from "../lib/api";
import { accountText } from "../lib/gog";
import { t } from "../lib/i18n";
import { LauncherIcon } from "./LauncherIcon";

/**
 * Section « Comptes » des Paramètres : compte GOG facultatif. « Se connecter » ouvre la page de
 * connexion de GOG dans une fenêtre à part (créée par Rust, sans accès à Ludex) ; aucun jeton ne
 * vient jusqu'ici, seulement l'état et le pseudo.
 */
export function AccountsPanel({ onToast }: { onToast?: (title: string) => void }) {
  const [account, setAccount] = useState<GogAccount | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const c = t.settings.accounts;

  useEffect(() => {
    let alive = true;
    gogAccount()
      .then((a) => alive && setAccount(a))
      .catch((e) => alive && setError(String(e)));
    let was: GogAccount["state"] | null = null;
    const off = onGogAccount((a) => {
      setAccount(a);
      setError(a.error);
      if (was !== null && was !== "connected" && a.state === "connected") onToast?.(c.signedIn);
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
  return (
    <section className="set__group" aria-labelledby="set-accounts">
      <h3 id="set-accounts">{c.title}</h3>
      <div className="set__row">
        <span className="set__text">
          <span className="with-icon"><LauncherIcon platform="gog" size={16} />{c.gogName}</span>
          <small id="set-gog-help">
            {account ? accountText(account) : "…"}. {c.gogHelp}
          </small>
        </span>
        <div className="set__acts">
          {state === "connected" ? (
            <button type="button" className="btn-ghost set__small" data-nav disabled={busy} title={c.disconnectHint} onClick={() => run(gogLogout, c.signedOut)}>
              {c.disconnect}
            </button>
          ) : (
            <>
              <button type="button" className="btn-ghost set__small is-primary" data-nav disabled={busy} aria-describedby="set-gog-help" onClick={() => run(gogLogin)}>
                {account?.loginOpen ? c.showWindow : state === "expired" ? c.reconnect : c.connect}
              </button>
              {state === "expired" && (
                <button type="button" className="btn-ghost set__small" data-nav disabled={busy} title={c.disconnectHint} onClick={() => run(gogLogout, c.signedOut)}>
                  {c.disconnect}
                </button>
              )}
            </>
          )}
        </div>
      </div>
      {error && <p className="set__note gog__error" role="alert">{error}</p>}
    </section>
  );
}
