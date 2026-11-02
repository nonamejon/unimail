import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Archive, Reply, Star, Trash2, MailOpen, Forward, ChevronDown } from "lucide-react";
import type { Account, Message } from "../api";
import { Avatar, senderIcon } from "./Avatar";
import { relTime } from "./MailList";
import { openUrl } from "@tauri-apps/plugin-opener";
import "./Reader.css";

interface Props {
  /** All messages in the open conversation, newest first. */
  thread: Message[];
  account: Account | null;
  onStar: (m: Message, starred: boolean) => void;
  /** Delete one message. */
  onDelete: (m: Message) => void;
  /** Archive the whole conversation. */
  onArchiveAll: () => void;
}

/** Plain-text bodies longer than this start clipped with a "Show more" control. */
const CLIP_CHARS = 420;

export function Reader({ thread, account, onStar, onDelete, onArchiveAll }: Props) {
  const newest = thread[0];
  const [open, setOpen] = useState<Set<number>>(new Set());
  useEffect(() => {
    setOpen(new Set(newest ? [newest.id] : []));
  }, [newest?.accountId, newest?.from.email, newest?.threadId]);

  if (!newest || !account) {
    return (
      <section className="reader reader-empty">
        <div className="reader-empty-art">
          <MailOpen size={38} strokeWidth={1.6} />
        </div>
        <p>Pick a message to read it here.</p>
      </section>
    );
  }

  const toggle = (id: number) =>
    setOpen((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const primary = newest.from;
  const anyStarred = thread.some((m) => m.starred);

  return (
    <section className="reader">
      <header className="reader-bar">
        <div className={"reader-account hue-" + account.hue}>
          <Avatar name={account.name} hue={account.hue} size={22} src={account.avatar} />
          <span>{account.email}</span>
        </div>
        <div className="reader-actions">
          <button className="icon-btn" title="Reply"><Reply size={18} /></button>
          <button className="icon-btn" title="Forward"><Forward size={18} /></button>
          <button className="icon-btn" title={anyStarred ? "Unstar" : "Star"} data-on={anyStarred} onClick={() => onStar(newest, !anyStarred)}>
            <Star size={18} fill={anyStarred ? "currentColor" : "none"} />
          </button>
          <button className="icon-btn" title="Archive conversation" onClick={onArchiveAll}><Archive size={18} /></button>
        </div>
      </header>

      <div className="thread">
        <div className="thread-head">
          <div className="thread-from">
            <Avatar name={primary.name} hue={account.hue} size={44} src={senderIcon(primary.email)} />
            <div className="thread-from-text">
              <div className="thread-from-name">{primary.name}</div>
              <div className="thread-from-email">{primary.email}</div>
            </div>
            {thread.length > 1 && <span className="thread-count">{thread.length} messages</span>}
          </div>
        </div>

        {thread.map((m) => (
          <MessageCard key={m.id} message={m} expanded={open.has(m.id)} onToggle={() => toggle(m.id)} onDelete={onDelete} />
        ))}
      </div>
    </section>
  );
}

interface CardProps {
  message: Message;
  expanded: boolean;
  onToggle: () => void;
  onDelete: (m: Message) => void;
}

const CLIP_HEIGHT = "9.5em";

function MessageCard({ message: m, expanded, onToggle, onDelete }: CardProps) {
  const [showAll, setShowAll] = useState(false);
  const isHtml = m.html.length > 0;
  const long = !isHtml && m.body.length > CLIP_CHARS;
  const bodyRef = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = bodyRef.current;
    if (!el || !long) return;
    el.style.maxHeight = showAll ? el.scrollHeight + "px" : CLIP_HEIGHT;
  }, [showAll, long]);
  const date = new Date(m.date);
  const fullDate =
    date.toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" }) +
    " · " +
    date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });

  return (
    <article className="msg" data-expanded={expanded}>
      <div className="msg-head-row">
        <button className="msg-head" onClick={onToggle}>
          <div className="msg-who">
            <span className="msg-subject">{m.subject}</span>
            {expanded
              ? <span className="msg-when">{fullDate}</span>
              : <span className="msg-snippet">{m.preview}</span>}
          </div>
          <span className="msg-when">{relTime(m.date)}</span>
          <ChevronDown size={16} className="msg-chevron" />
        </button>
        <div className="msg-tools">
          <button className="icon-btn" title="Delete this message" onClick={() => onDelete(m)}><Trash2 size={16} /></button>
        </div>
      </div>

      <div className="msg-collapse" aria-hidden={!expanded}>
        <div>
          <div className="msg-body-wrap" data-clipped={long && !showAll}>
            {isHtml
              ? (expanded ? <HtmlBody html={m.html} /> : null)
              : <div className="msg-body" ref={bodyRef} data-long={long}>{linkify(m.body)}</div>}
            {long && (
              <button className="msg-more" onClick={() => setShowAll((v) => !v)} tabIndex={expanded ? 0 : -1}>
                {showAll ? "Show less" : "Show more"}
              </button>
            )}
          </div>
        </div>
      </div>
    </article>
  );
}

/** Turn bare URLs in plain text into links that open in the system browser. */
function linkify(text: string) {
  const parts = text.split(/(https?:\/\/[^\s<>"')\]]+)/g);
  return parts.map((part, i) =>
    i % 2 === 1
      ? <a key={i} href={part} onClick={(e) => { e.preventDefault(); openUrl(part).catch(() => {}); }}>{part}</a>
      : part,
  );
}

/** Renders sanitized email HTML in an isolated, script-free frame sized to its content. */
function HtmlBody({ html }: { html: string }) {
  const ref = useRef<HTMLIFrameElement>(null);
  const [height, setHeight] = useState(0);
  const [ready, setReady] = useState(false);

  const doc =
    "<!doctype html><html><head><meta charset='utf-8'>" +
    // No scripts, no frames, no forms; images and inline CSS only.
    "<meta http-equiv='Content-Security-Policy' content=\"default-src 'none'; img-src https: http: data: cid:; style-src 'unsafe-inline'; font-src https: data:;\">" +
    "<base target='_blank'>" +
    "<style>" +
    "@font-face{font-family:Poppins;font-weight:400;src:url('/fonts/poppins-400.woff2') format('woff2')}" +
    "@font-face{font-family:Poppins;font-weight:600;src:url('/fonts/poppins-600.woff2') format('woff2')}" +
    // Padding lives on <html>, which emails never style, so their own body{padding:0} can't remove it.
    // overflow hidden: the frame is sized to its content by the app, it never scrolls itself.
    "html{margin:0;padding:20px 20px 24px !important;background:#fff;overflow:hidden}" +
    "body{margin:0;padding:0;font-family:Poppins,'Segoe UI',sans-serif;font-size:14px;line-height:1.55;overflow-wrap:anywhere;" +
    "background:#fff;color:#202124}" +
    "a{color:#1a56c4} img{max-width:100%;height:auto} table{max-width:100%}" +
    "</style></head><body>" + html + "</body></html>";

  // Size the frame as soon as its document is parsed (not after every image has loaded),
  // then keep tracking it as images and fonts arrive.
  useEffect(() => {
    setReady(false);
    const frame = ref.current;
    if (!frame) return;
    let ro: ResizeObserver | null = null;
    let raf = 0;
    let attached = false;
    const attach = () => {
      if (attached) return;
      const d = frame.contentDocument;
      // srcdoc documents start as about:blank; wait for ours (it has our <base> tag).
      if (!d?.documentElement || d.readyState === "loading" || !d.querySelector("base")) return;
      attached = true;
      // Links: the frame can't open windows, so hand every link to the system browser.
      d.addEventListener("click", (ev) => {
        const a = (ev.target as Element | null)?.closest("a[href]") as HTMLAnchorElement | null;
        if (!a) return;
        ev.preventDefault();
        const href = a.getAttribute("href") ?? "";
        if (/^(https?:|mailto:)/i.test(href)) openUrl(href).catch(() => {});
      });
      const measure = () => setHeight(Math.min(20000, Math.max(24, d.documentElement.scrollHeight)));
      measure();
      setReady(true);
      ro = new ResizeObserver(measure);
      ro.observe(d.documentElement);
      if (d.body) ro.observe(d.body);
    };
    const poll = () => { attach(); if (!attached) raf = requestAnimationFrame(poll); };
    raf = requestAnimationFrame(poll);
    frame.addEventListener("load", attach);
    return () => { cancelAnimationFrame(raf); frame.removeEventListener("load", attach); ro?.disconnect(); };
  }, [html]);

  return (
    <iframe
      ref={ref}
      className="msg-html"
      data-ready={ready}
      title="Message"
      scrolling="no"
      sandbox="allow-same-origin allow-popups allow-popups-to-escape-sandbox"
      srcDoc={doc}
      style={{ height }}
    />
  );
}
