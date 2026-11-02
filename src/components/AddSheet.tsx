import { useEffect, useState } from "react";
import { X, Link2, Mail, ArrowRight, ShieldCheck, ArrowLeft, Loader2, ExternalLink, Globe, Check } from "lucide-react";
import { api, type Account, type LinkStatus } from "../api";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Avatar } from "./Avatar";
import { ProviderMark } from "./ProviderMark";
import "./AddSheet.css";

interface Props {
  accounts: Account[];
  onClose: () => void;
  onAdded: (account: Account) => void;
  /** Bump to ask the sheet to close (with its exit animation). */
  closeSignal: number;
  /** A Google sign-in link handed over by Windows; opens straight on the chooser. */
  initialUrl?: string | null;
}

type Tab = "signin" | "add";
type Provider = "gmail" | "outlook" | "icloud" | "imap";

function looksLikeGoogleAuth(url: string) {
  try {
    const u = new URL(url.trim());
    return u.hostname === "accounts.google.com" && u.pathname.includes("/o/oauth2");
  } catch {
    return false;
  }
}

const EXIT_MS = 200;

export function AddSheet({ accounts, onClose, onAdded, closeSignal, initialUrl }: Props) {
  const [closing, setClosing] = useState(false);
  const close = () => {
    if (closing) return;
    setClosing(true);
    window.setTimeout(onClose, EXIT_MS);
  };
  useEffect(() => { if (closeSignal > 0) close(); }, [closeSignal]);

  const [tab, setTab] = useState<Tab>(initialUrl || accounts.length ? "signin" : "add");
  const [url, setUrl] = useState(initialUrl ?? "");
  const [links, setLinks] = useState<LinkStatus | null>(null);
  useEffect(() => { api.linkStatus().then(setLinks).catch(() => {}); }, []);
  const valid = looksLikeGoogleAuth(url);
  const google = accounts.filter((a) => a.provider === "gmail");

  // add-account form
  const [provider, setProvider] = useState<Provider | null>(null);
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const account = await api.addGmail(name, email, password);
      onAdded(account);
      close();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="sheet-backdrop" data-closing={closing} onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className="sheet" data-closing={closing} role="dialog" aria-modal>
        <header className="sheet-head">
          <div className="sheet-tabs">
            <button className="pill" data-active={tab === "signin"} onClick={() => setTab("signin")}>
              <ShieldCheck size={15} /> Sign in with Google
            </button>
            <button className="pill" data-active={tab === "add"} onClick={() => setTab("add")}>
              <Mail size={15} /> Add account
            </button>
          </div>
          <button className="icon-btn" onClick={close} title="Close"><X size={18} /></button>
        </header>

        {tab === "signin" && (
          <div className="sheet-body" key="signin">
            <h2 className="sheet-title">Which account should sign in?</h2>
            <p className="sheet-hint">
              Paste the Google sign-in link a site or app just opened. UniMail finishes the login with the account you pick and
              hands the result back to your browser, so nothing else changes.
            </p>
            <label className="sheet-field" data-valid={url ? valid : undefined}>
              <Link2 size={16} />
              <input
                autoFocus
                placeholder="https://accounts.google.com/o/oauth2/..."
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                spellCheck={false}
              />
            </label>
            {google.length === 0 && <p className="sheet-hint">Connect a Gmail account first, then it shows up here.</p>}
            {error && <p className="sheet-error">{error}</p>}
            <div className="sheet-grid" data-disabled={!valid}>
              {google.map((a) => (
                <button
                  key={a.id}
                  className="acct"
                  disabled={!valid}
                  title={valid ? "Sign in as " + a.email : "Paste a Google sign-in link first"}
                  onClick={() => api.openGoogle(a.email, url.trim()).then(close).catch((e) => setError(String(e)))}
                >
                  <Avatar name={a.name} hue={a.hue} size={40} src={a.avatar} />
                  <div className="acct-text">
                    <div className="acct-name">{a.name}</div>
                    <div className="acct-email">{a.email}</div>
                  </div>
                </button>
              ))}
            </div>

            <div className="sheet-links">
              <div className="sheet-links-row">
                <Globe size={15} />
                <span>Other links open in</span>
                <select
                  className="sheet-select"
                  value={links?.browser ?? ""}
                  onChange={(e) => { api.setLinkBrowser(e.target.value); setLinks((l) => (l ? { ...l, browser: e.target.value } : l)); }}
                >
                  {!links?.browser && <option value="">choose a browser</option>}
                  {links?.browsers.map((b) => <option key={b.path} value={b.path}>{b.name}</option>)}
                </select>
              </div>
              <div className="sheet-links-row">
                {links?.registered ? <Check size={15} /> : <ExternalLink size={15} />}
                <span>{links?.registered ? "Registered as a browser. Pick UniMail under Web browser in Windows Settings." : "Let sites and apps send their Google sign-in here."}</span>
                <button className="sheet-mini" onClick={() => api.makeDefaultBrowser().then(() => setLinks((l) => (l ? { ...l, registered: true } : l))).catch((e) => setError(String(e)))}>
                  {links?.registered ? "Open Settings" : "Make default browser"}
                </button>
              </div>
            </div>
          </div>
        )}

        {tab === "add" && provider === null && (
          <div className="sheet-body" key="add">
            <h2 className="sheet-title">Connect a mailbox</h2>
            <p className="sheet-hint">It stays signed in until you remove it. Nothing leaves your computer.</p>
            <div className="providers">
              {([
                ["gmail", "Gmail"],
                ["outlook", "Outlook"],
                ["icloud", "iCloud"],
                ["imap", "Other IMAP"],
              ] as const).map(([id, label]) => (
                <button key={id} className="provider" disabled={id !== "gmail"} onClick={() => setProvider(id)}>
                  <ProviderMark id={id} />
                  {label}
                  {id !== "gmail" && <span className="provider-soon">soon</span>}
                </button>
              ))}
            </div>
          </div>
        )}

        {tab === "add" && provider === "gmail" && (
          <div className="sheet-body" key="gmail">
            <button className="sheet-back" onClick={() => setProvider(null)}><ArrowLeft size={15} /> Providers</button>
            <h2 className="sheet-title">Gmail</h2>
            <p className="sheet-hint">
              Google needs an <strong>app password</strong> for mail apps. Create one in your browser (2-Step Verification
              has to be on), paste it here, done. It only works for mail and can be revoked anytime.
            </p>
            <button
              className="sheet-google"
              onClick={() => openUrl("https://myaccount.google.com/apppasswords").catch((e) => setError(String(e)))}
            >
              <ExternalLink size={15} /> Open the app passwords page
            </button>
            <label className="sheet-field">
              <Mail size={16} />
              <input autoFocus placeholder="you@gmail.com" value={email} onChange={(e) => setEmail(e.target.value)} spellCheck={false} />
            </label>
            <label className="sheet-field">
              <ShieldCheck size={16} />
              <input
                placeholder="16-letter app password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                spellCheck={false}
                type="password"
                onKeyDown={(e) => e.key === "Enter" && submit()}
              />
            </label>
            <label className="sheet-field">
              <span className="sheet-field-label">Name</span>
              <input placeholder="Optional, you can change it later" value={name} onChange={(e) => setName(e.target.value)} />
            </label>
            {error && <p className="sheet-error">{error}</p>}
            <button className="primary" disabled={busy || !email || !password} onClick={submit}>
              {busy ? <><Loader2 size={16} className="spin" /> Signing in and downloading your inbox…</> : <>Connect <ArrowRight size={16} /></>}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
