import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, type Account, type Folder, type Hue, type Message } from "./api";
import { open as pickFile } from "@tauri-apps/plugin-dialog";
import { AccountRail } from "./components/AccountRail";
import { MailList, type Row } from "./components/MailList";
import { Reader } from "./components/Reader";
import { AddSheet } from "./components/AddSheet";
import { WindowControls, useMaximized, useWindowDrag } from "./components/WindowControls";
import "./styles/base.css";
import "./App.css";

/** Background poll interval while the app is open. */
const POLL_MS = 90_000;

/**
 * A conversation is "everything this sender sent me" in an account, like a chat with a person.
 * Sent mail has no single counterpart, so it groups by reply thread instead.
 */
type Conversation = { accountId: string; kind: "sender"; sender: string } | { accountId: string; kind: "thread"; threadId: string };
const convKey = (c: Conversation) => (c.kind === "sender" ? c.accountId + " s " + c.sender.toLowerCase() : c.accountId + " t " + c.threadId);
const convOf = (m: Message, folder: Folder): Conversation =>
  folder === "sent" ? { accountId: m.accountId, kind: "thread", threadId: m.threadId } : { accountId: m.accountId, kind: "sender", sender: m.from.email };

export default function App() {
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [messages, setMessages] = useState<Message[]>([]);
  const [selectedAccount, setSelectedAccount] = useState<string | "all">("all");
  const [folder, setFolder] = useState<Folder>("inbox");
  const [selected, setSelected] = useState<Conversation | null>(null);
  const [thread, setThread] = useState<Message[]>([]);
  const [syncing, setSyncing] = useState(false);
  const [sheet, setSheet] = useState(false);
  const [sheetClose, setSheetClose] = useState(0);
  /** A Google sign-in link handed to us by Windows; opens the sheet on the chooser with it filled in. */
  const [sheetUrl, setSheetUrl] = useState<string | null>(null);
  const [refreshTick, setRefreshTick] = useState(0);
  const [renameSignal, setRenameSignal] = useState(0);
  const [query, setQuery] = useState("");
  /** Accounts whose last sync failed (usually a revoked app password), with the reason. */
  const [accountErrors, setAccountErrors] = useState<Record<string, string>>({});
  const sheetRef = useRef(false);
  sheetRef.current = sheet;
  const maximized = useMaximized();
  useWindowDrag(maximized);

  // ---- data loading
  const reload = useCallback(async () => {
    const [a, m] = await Promise.all([api.listAccounts(), api.listMessages()]);
    setAccounts(a);
    setMessages(m);
  }, []);

  const sync = useCallback(async () => {
    setSyncing(true);
    try {
      const reports = await api.sync();
      setAccountErrors(Object.fromEntries(reports.filter((r) => r.error).map((r) => [r.accountId, r.error as string])));
    } catch { /* keep the previous state */ }
    finally { setSyncing(false); }
  }, []);

  useEffect(() => {
    reload();
    sync();
    const unlisten = api.onUpdated(() => reload());
    const timer = window.setInterval(sync, POLL_MS);
    return () => { unlisten.then((f) => f()); window.clearInterval(timer); };
  }, [reload, sync]);

  // Load the full conversation (with bodies) whenever the selection changes, and mark it read.
  useEffect(() => {
    if (!selected) { setThread([]); return; }
    let cancelled = false;
    const load = selected.kind === "sender"
      ? api.getSenderThread(selected.accountId, selected.sender)
      : api.getThread(selected.accountId, selected.threadId);
    load.then((t) => {
      if (cancelled) return;
      const unread = t.filter((m) => m.unread);
      setThread(t.map((m) => ({ ...m, unread: false })));
      if (unread.length) {
        unread.forEach((m) => api.markRead(m.id, true));
        const ids = new Set(unread.map((u) => u.id));
        const inboxUnread = unread.filter((u) => u.folder === "inbox").length;
        setMessages((prev) => prev.map((m) => (ids.has(m.id) ? { ...m, unread: false } : m)));
        setAccounts((prev) => prev.map((a) => (a.id === selected.accountId ? { ...a, unread: Math.max(0, a.unread - inboxUnread) } : a)));
      }
    });
    return () => { cancelled = true; };
  }, [selected]);

  // ---- keys: Esc closes the sheet, then the email; F5 / Ctrl+R is a refresh, never a reload
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const inField = (e.target as HTMLElement | null)?.closest("input, textarea, [contenteditable]");
      if (e.key === "Escape") {
        if (sheetRef.current) { setSheetClose((n) => n + 1); return; }
        if (!inField) { setSelected(null); return; }
      }
      const isReload = e.key === "F5" || ((e.ctrlKey || e.metaKey) && (e.key === "r" || e.key === "R"));
      const nav = e.altKey && (e.key === "ArrowLeft" || e.key === "ArrowRight");
      if (isReload) {
        e.preventDefault();
        setRefreshTick((t) => t + 1);
        sync();
        return;
      }
      if (nav || e.key === "BrowserBack" || e.key === "BrowserForward") e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [sync]);

  // ---- derived
  const accountsById = useMemo(() => Object.fromEntries(accounts.map((a) => [a.id, a])) as Record<string, Account>, [accounts]);

  const scoped = useMemo(() => {
    const q = query.trim().toLowerCase();
    return messages
      .filter((m) => selectedAccount === "all" || m.accountId === selectedAccount)
      .filter((m) => {
        if (folder === "starred") return m.starred;
        if (folder === "archive") return false; // not cached yet
        return m.folder === folder;
      })
      .filter((m) => !q || m.subject.toLowerCase().includes(q) || m.from.name.toLowerCase().includes(q) || m.from.email.toLowerCase().includes(q) || m.preview.toLowerCase().includes(q));
  }, [messages, selectedAccount, folder, query]);

  const rows = useMemo<Row[]>(() => {
    const seen = new Map<string, Row>();
    for (const m of scoped) {
      const k = convKey(convOf(m, folder));
      const t = seen.get(k);
      if (t) { t.count += 1; t.unread ||= m.unread; }
      else seen.set(k, { key: k, latest: m, count: 1, unread: m.unread });
    }
    return [...seen.values()];
  }, [scoped, folder]);

  const account = selected ? accountsById[selected.accountId] ?? null : null;
  const current = selectedAccount === "all" ? null : accountsById[selectedAccount] ?? null;
  const title = selectedAccount === "all" ? "All Inboxes" : current?.name ?? "";

  // ---- actions
  const onStar = async (m: Message, starred: boolean) => {
    setThread((prev) => prev.map((x) => (x.id === m.id ? { ...x, starred } : x)));
    setMessages((prev) => prev.map((x) => (x.id === m.id ? { ...x, starred } : x)));
    await api.setStarred(m.id, starred);
  };
  const onDelete = async (m: Message) => {
    setThread((prev) => {
      const next = prev.filter((x) => x.id !== m.id);
      if (next.length === 0) setSelected(null);
      return next;
    });
    setMessages((prev) => prev.filter((x) => x.id !== m.id));
    await api.move(m.id, "trash");
  };
  const onArchiveAll = async () => {
    const ids = thread.map((m) => m.id);
    setSelected(null);
    setMessages((prev) => prev.filter((m) => !ids.includes(m.id)));
    await Promise.all(ids.map((id) => api.move(id, "archive")));
  };
  const onRename = async (id: string, name: string) => {
    setAccounts((prev) => prev.map((a) => (a.id === id ? { ...a, name } : a)));
    await api.renameAccount(id, name);
  };
  const onHue = async (id: string, hue: Hue) => {
    setAccounts((prev) => prev.map((a) => (a.id === id ? { ...a, hue } : a)));
    await api.setAccountHue(id, hue);
  };
  // Custom account picture: pick a file, square-crop and shrink it to 128px, store as a data URL.
  const onPicture = async (id: string) => {
    const path = await pickFile({ multiple: false, directory: false, filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp"] }] });
    if (!path || typeof path !== "string") return;
    const src = await api.readImage(path);
    const dataUrl = await new Promise<string>((resolve, reject) => {
      const img = new Image();
      img.onload = () => {
        const side = Math.min(img.width, img.height);
        const c = document.createElement("canvas");
        c.width = 128; c.height = 128;
        const ctx = c.getContext("2d")!;
        ctx.drawImage(img, (img.width - side) / 2, (img.height - side) / 2, side, side, 0, 0, 128, 128);
        resolve(c.toDataURL("image/png"));
      };
      img.onerror = () => reject(new Error("Could not read that image."));
      img.src = src;
    });
    setAccounts((prev) => prev.map((a) => (a.id === id ? { ...a, avatar: dataUrl } : a)));
    await api.setAccountPicture(id, dataUrl);
  };
  const onClearPicture = async (id: string) => {
    setAccounts((prev) => prev.map((a) => (a.id === id ? { ...a, avatar: null } : a)));
    await api.setAccountPicture(id, null);
  };
  const onRemove = async (id: string) => {
    if (selectedAccount === id) setSelectedAccount("all");
    if (selected?.accountId === id) setSelected(null);
    setAccounts((prev) => prev.filter((a) => a.id !== id));
    setMessages((prev) => prev.filter((m) => m.accountId !== id));
    await api.removeAccount(id);
  };

  return (
    <div className="frame" data-maximized={maximized}>
      <div className="app">
        <WindowControls maximized={maximized} />
        <AccountRail
          accounts={accounts}
          selected={selectedAccount}
          onSelect={setSelectedAccount}
          onAdd={() => setSheet(true)}
          onRename={(id) => { setSelectedAccount(id); setRenameSignal((n) => n + 1); }}
          onHue={onHue}
          onRemove={onRemove}
          onPicture={onPicture}
          onClearPicture={onClearPicture}
          errors={accountErrors}
        />
        <main className="main">
          <MailList
            viewKey={selectedAccount + ":" + folder + ":" + refreshTick + ":" + (query.trim() ? "q" : "")}
            title={title}
            syncing={syncing}
            accountsById={accountsById}
            rows={rows}
            folder={folder}
            onFolder={setFolder}
            selectedKey={selected ? convKey(selected) : null}
            onSelect={(row) => setSelected(convOf(row.latest, folder))}
            showAccountChip={selectedAccount === "all"}
            hasAccounts={accounts.length > 0}
            onAdd={() => setSheet(true)}
            renameTarget={current ? { id: current.id, name: current.name } : null}
            renameSignal={renameSignal}
            onRename={onRename}
            query={query}
            onQuery={setQuery}
          />
          <Reader thread={thread} account={account} onStar={onStar} onDelete={onDelete} onArchiveAll={onArchiveAll} />
        </main>
        {sheet && (
          <AddSheet
            accounts={accounts}
            initialUrl={sheetUrl}
            closeSignal={sheetClose}
            onClose={() => { setSheet(false); setSheetClose(0); setSheetUrl(null); }}
            onAdded={() => reload()}
          />
        )}
      </div>
    </div>
  );
}
