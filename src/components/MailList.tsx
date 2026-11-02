import { useEffect, useRef, useState } from "react";
import { Search, Star, Pencil, Loader2, Plus, Check } from "lucide-react";
import type { Account, Folder, Message } from "../api";
import "./MailList.css";

export interface Row { key: string; latest: Message; count: number; unread: boolean }

interface Props {
  /** Changes whenever the account or folder changes; remounts the rows so they always animate in. */
  viewKey: string;
  title: string;
  syncing: boolean;
  accountsById: Record<string, Account>;
  rows: Row[];
  folder: Folder;
  onFolder: (f: Folder) => void;
  selectedKey: string | null;
  onSelect: (row: Row) => void;
  showAccountChip: boolean;
  hasAccounts: boolean;
  onAdd: () => void;
  /** Set when a single account is selected: lets the header rename it. */
  renameTarget: { id: string; name: string } | null;
  renameSignal: number;
  onRename: (id: string, name: string) => void;
  query: string;
  onQuery: (q: string) => void;
}

const folders: { id: Folder; label: string }[] = [
  { id: "inbox", label: "Inbox" },
  { id: "starred", label: "Starred" },
  { id: "sent", label: "Sent" },
  { id: "archive", label: "Archive" },
];

export function relTime(iso: string) {
  const d = new Date(iso);
  const diff = (Date.now() - d.getTime()) / 60000;
  if (diff < 1) return "now";
  if (diff < 60) return Math.round(diff) + "m";
  if (diff < 60 * 24) return Math.round(diff / 60) + "h";
  if (diff < 60 * 24 * 7) return Math.round(diff / 1440) + "d";
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

export function MailList(props: Props) {
  const { viewKey, title, syncing, accountsById, rows, folder, onFolder, selectedKey, onSelect, showAccountChip, hasAccounts, onAdd, renameTarget, renameSignal, onRename, query, onQuery } = props;
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  const startEdit = () => {
    if (!renameTarget) return;
    setDraft(renameTarget.name);
    setEditing(true);
  };
  useEffect(() => { if (renameSignal > 0) startEdit(); }, [renameSignal]);
  useEffect(() => { setEditing(false); }, [renameTarget?.id]);
  useEffect(() => { if (editing) { inputRef.current?.focus(); inputRef.current?.select(); } }, [editing]);

  const commit = () => {
    setEditing(false);
    if (renameTarget && draft.trim() && draft.trim() !== renameTarget.name) onRename(renameTarget.id, draft.trim());
  };

  return (
    <section className="list">
      <header className="list-head">
        <div className="list-title-row">
          {editing ? (
            <input
              ref={inputRef}
              className="list-title-input"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter") commit(); if (e.key === "Escape") setEditing(false); }}
              onBlur={commit}
              maxLength={40}
            />
          ) : (
            <h1 className="list-title">{title}</h1>
          )}
          <div className="list-title-actions">
            {syncing && <Loader2 size={16} className="list-spin" />}
            {renameTarget && !editing && (
              <button className="icon-btn" title="Rename this account" onClick={startEdit}><Pencil size={17} /></button>
            )}
            {editing && (
              <button className="icon-btn" title="Save" onMouseDown={(e) => e.preventDefault()} onClick={commit}><Check size={18} /></button>
            )}
          </div>
        </div>
        <label className="search">
          <Search size={16} />
          <input
            placeholder="Search all mail"
            value={query}
            onChange={(e) => onQuery(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Escape") { onQuery(""); (e.target as HTMLInputElement).blur(); } }}
          />
          {query && <button className="search-clear" onClick={() => onQuery("")} title="Clear">×</button>}
        </label>
        <div className="folders">
          {folders.map((f) => (
            <button key={f.id} className="pill" data-active={folder === f.id} onClick={() => onFolder(f.id)}>
              {f.label}
            </button>
          ))}
        </div>
      </header>

      <div className="list-scroll" key={viewKey}>
        {!hasAccounts && (
          <div className="list-empty">
            <p>No accounts yet.</p>
            <button className="list-empty-btn" onClick={onAdd}><Plus size={16} /> Add an account</button>
          </div>
        )}
        {hasAccounts && rows.length === 0 && <div className="list-empty">{query ? "No matches." : "Nothing here."}</div>}
        {rows.map((row) => {
          const { latest: m, count, unread, key } = row;
          const acc = accountsById[m.accountId];
          return (
            <button key={key} className="row" data-selected={selectedKey === key} data-unread={unread} onClick={() => onSelect(row)}>
              <div className="row-body">
                <div className="row-top">
                  {unread && <span className="row-unread" aria-label="Unread" />}
                  <span className="row-from">{m.from.name}{count > 1 && <span className="row-count">{count}</span>}</span>
                  {showAccountChip && acc && (
                    <span className={"row-chip hue-" + acc.hue} title={acc.email}>{acc.name}</span>
                  )}
                  {m.starred && <Star size={13} className="row-star" fill="currentColor" />}
                  <span className="row-time">{relTime(m.date)}</span>
                </div>
                <div className="row-subject">{m.subject}</div>
                <div className="row-preview">{m.preview}</div>
              </div>
            </button>
          );
        })}
      </div>
    </section>
  );
}
