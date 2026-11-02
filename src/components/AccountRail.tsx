import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Inbox, Plus, Pencil, Trash2, ImagePlus, ImageOff } from "lucide-react";
import type { Account, Hue } from "../api";
import { Avatar } from "./Avatar";
import "./AccountRail.css";

const HUES: Hue[] = ["coral", "sun", "leaf", "sky", "grape", "mint", "rose", "slate"];

interface Props {
  accounts: Account[];
  selected: string | "all";
  onSelect: (id: string | "all") => void;
  onAdd: () => void;
  onRename: (id: string) => void;
  onHue: (id: string, hue: Hue) => void;
  onRemove: (id: string) => void;
  onPicture: (id: string) => void;
  onClearPicture: (id: string) => void;
  /** accountId -> reason its last sync failed */
  errors: Record<string, string>;
}

interface Menu { id: string; x: number; y: number }

export function AccountRail({ accounts, selected, onSelect, onAdd, onRename, onHue, onRemove, onPicture, onClearPicture, errors }: Props) {
  const totalUnread = accounts.reduce((n, a) => n + a.unread, 0);
  const listRef = useRef<HTMLDivElement>(null);
  const [glideY, setGlideY] = useState<number | null>(null);
  const [menu, setMenu] = useState<Menu | null>(null);

  // The highlight is one element that slides to whichever tile is selected.
  useLayoutEffect(() => {
    const el = listRef.current?.querySelector<HTMLElement>('[data-id="' + selected + '"]');
    setGlideY(el ? el.offsetTop : null);
  }, [selected, accounts.length]);

  // Close the context menu on any outside press or Escape.
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const key = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", key);
    return () => { window.removeEventListener("mousedown", close); window.removeEventListener("keydown", key); };
  }, [menu]);

  const menuAccount = menu ? accounts.find((a) => a.id === menu.id) : null;

  return (
    <nav className="rail">
      <div className="rail-list" ref={listRef}>
        {glideY !== null && <span className="rail-glide" style={{ transform: "translateY(" + glideY + "px)" }} />}

        <button className="rail-item" data-id="all" data-active={selected === "all"} onClick={() => onSelect("all")} title="All Inboxes">
          <Inbox size={36} strokeWidth={2.2} className="rail-all" />
          {totalUnread > 0 && <span className="rail-badge">{totalUnread > 99 ? "99+" : totalUnread}</span>}
        </button>

        {accounts.map((a) => (
          <button
            key={a.id}
            className="rail-item"
            data-id={a.id}
            data-active={selected === a.id}
            onClick={() => onSelect(a.id)}
            onContextMenu={(e) => { e.preventDefault(); setMenu({ id: a.id, x: e.clientX, y: e.clientY }); }}
            title={errors[a.id] ? a.email + " can't sign in. Add it again with a new app password." : a.name + " · " + a.email}
          >
            <Avatar name={a.name} hue={a.hue} size={36} src={a.avatar} />
            {errors[a.id]
              ? <span className="rail-badge rail-badge-error">!</span>
              : a.unread > 0 && <span className="rail-badge">{a.unread > 99 ? "99+" : a.unread}</span>}
          </button>
        ))}

        <button className="rail-item rail-add" onClick={onAdd} title="Add account or sign in with Google">
          <Plus size={52} strokeWidth={2} className="rail-plus" />
        </button>
      </div>

      {menu && menuAccount && (
        <div className="ctx" style={{ left: menu.x, top: menu.y }} onMouseDown={(e) => e.stopPropagation()}>
          <div className="ctx-title">{menuAccount.email}</div>
          <div className="ctx-hues">
            {HUES.map((h) => (
              <button
                key={h}
                className={"ctx-hue hue-" + h}
                data-on={menuAccount.hue === h}
                title={h}
                onClick={() => { onHue(menuAccount.id, h); }}
              />
            ))}
          </div>
          <button className="ctx-item" onClick={() => { setMenu(null); onRename(menuAccount.id); }}>
            <Pencil size={14} /> Rename
          </button>
          <button className="ctx-item" onClick={() => { setMenu(null); onPicture(menuAccount.id); }}>
            <ImagePlus size={14} /> {menuAccount.avatar ? "Change picture…" : "Set a picture…"}
          </button>
          {menuAccount.avatar && (
            <button className="ctx-item" onClick={() => { setMenu(null); onClearPicture(menuAccount.id); }}>
              <ImageOff size={14} /> Remove picture
            </button>
          )}
          <button className="ctx-item ctx-danger" onClick={() => { setMenu(null); onRemove(menuAccount.id); }}>
            <Trash2 size={14} /> Remove account
          </button>
        </div>
      )}
    </nav>
  );
}
