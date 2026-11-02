import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { X } from "lucide-react";
import { api, type Account } from "../api";
import { Avatar } from "./Avatar";
import "../styles/base.css";
import "./Chooser.css";

/**
 * The account picker window. Opens when a site or app hands Windows a Google sign-in link.
 * One click on an account finishes the login as that account, no confirm step.
 */
export function Chooser({ url }: { url: string }) {
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const win = getCurrentWindow();

  useEffect(() => {
    api.listAccounts().then((a) => setAccounts(a.filter((x) => x.provider === "gmail"))).catch(() => {});
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") win.close(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Who is asking, when the link says (redirect_uri host, or the app's own name if Google passes one).
  const asker = (() => {
    try {
      const u = new URL(url);
      const r = u.searchParams.get("redirect_uri");
      if (r) {
        const h = new URL(r).hostname;
        return h === "localhost" || h === "127.0.0.1" ? "an app on this computer" : h;
      }
    } catch { /* fall through */ }
    return null;
  })();

  const pick = async (a: Account) => {
    if (busy) return;
    setBusy(a.id);
    try {
      await api.openGoogle(a.email, url);
      win.close();
    } catch {
      setBusy(null);
    }
  };

  return (
    <div className="chooser">
      <div className="chooser-head">
        <div className="chooser-titles">
          <div className="chooser-title">Sign in with Google</div>
          <div className="chooser-sub">{asker ? "to continue to " + asker : "pick the account to use"}</div>
        </div>
        <button className="icon-btn" title="Cancel" onClick={() => win.close()}><X size={18} /></button>
      </div>

      <div className="chooser-list">
        {accounts.map((a) => (
          <button key={a.id} className="chooser-acct" data-busy={busy === a.id} onClick={() => pick(a)}>
            <Avatar name={a.name} hue={a.hue} size={40} src={a.avatar} />
            <div className="chooser-text">
              <div className="chooser-name">{a.name}</div>
              <div className="chooser-email">{a.email}</div>
            </div>
          </button>
        ))}
        {accounts.length === 0 && <div className="chooser-empty">No Gmail accounts connected yet.</div>}
      </div>

      <div className="chooser-foot">The account you pick signs in through its own saved Google session inside UniMail.</div>
    </div>
  );
}
