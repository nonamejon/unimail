import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { Minus, Square, Copy, X } from "lucide-react";
import "./WindowControls.css";

const win = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window ? getCurrentWindow() : null;

/** Tracks whether the native window is maximized, so the frame can drop its rounded corners. */
export function useMaximized() {
  const [max, setMax] = useState(false);
  useEffect(() => {
    if (!win) return;
    let un: (() => void) | undefined;
    win.isMaximized().then(setMax);
    win.onResized(() => win.isMaximized().then(setMax)).then((u) => (un = u));
    return () => un?.();
  }, []);
  return max;
}

export function WindowControls({ maximized }: { maximized: boolean }) {
  if (!win) return null;
  return (
    <div className="winctl">
      <button className="winctl-btn" onClick={() => win.minimize()} title="Minimize"><Minus size={14} strokeWidth={2.4} /></button>
      <button className="winctl-btn" onClick={() => invoke("toggle_maximize_animated")} title={maximized ? "Restore" : "Maximize"}>
        {maximized ? <Copy size={12} strokeWidth={2.2} /> : <Square size={12} strokeWidth={2.4} />}
      </button>
      <button className="winctl-btn winctl-close" onClick={() => win.hide()} title="Close (UniMail keeps running in the tray)"><X size={15} strokeWidth={2.4} /></button>
    </div>
  );
}

/** Anything that should NOT start a window drag when pressed. */
const INTERACTIVE = "button, a, input, textarea, select, [contenteditable], .row, .acct, .provider, .reader-subject, .thread-from-text, .msg-body, .sheet-field, .sheet-backdrop, .ctx, .winctl";

/**
 * Press-and-drag on any empty spot moves the window; double-click on empty space toggles maximize.
 * Interactive elements and selectable text are left alone.
 */
/** When maximized, the pointer has to travel this far before a drag un-maximizes the window. */
const MAXIMIZED_DRAG_THRESHOLD = 44;

export function useWindowDrag(maximized: boolean) {
  const maxRef = useRef(maximized);
  maxRef.current = maximized;

  useEffect(() => {
    if (!win) return;
    const isEmpty = (e: MouseEvent) => {
      const t = e.target as HTMLElement | null;
      return !!t && e.button === 0 && !t.closest(INTERACTIVE);
    };
    const onDown = (e: MouseEvent) => {
      if (!isEmpty(e)) return;
      // A click on empty space while text is selected should just clear the selection, like any app.
      const sel = window.getSelection();
      if (sel && !sel.isCollapsed) { sel.removeAllRanges(); return; }
      if (e.detail >= 2) {
        // Double-click toggles maximize. Stop the browser from selecting text underneath,
        // otherwise the next press only clears that selection instead of dragging.
        e.preventDefault();
        if (e.detail === 2) invoke("toggle_maximize_animated");
        return;
      }
      if (!maxRef.current) { win.startDragging(); return; }

      // Maximized: arm a drag but only commit once the pointer clearly moves,
      // so a tiny wobble while clicking doesn't yank the window out of fullscreen.
      const sx = e.clientX, sy = e.clientY;
      // Where the grab is, as a fraction of the maximized window. The restored window is
      // placed so the cursor lands at the same fraction, like a native title bar does.
      const fx = sx / window.innerWidth;
      const grabY = sy * window.devicePixelRatio;
      const onMove = (ev: MouseEvent) => {
        if (Math.hypot(ev.clientX - sx, ev.clientY - sy) < MAXIMIZED_DRAG_THRESHOLD) return;
        cleanup();
        // One Rust call does unmaximize + place under cursor + start drag atomically.
        invoke("restore_and_drag", { fx, grabY }).catch(() => win.startDragging());
      };
      const cleanup = () => {
        document.removeEventListener("mousemove", onMove);
        document.removeEventListener("mouseup", cleanup);
      };
      document.addEventListener("mousemove", onMove);
      document.addEventListener("mouseup", cleanup);
    };
    // Suppress the engine's stock right-click menu. Text fields keep it for cut/copy/paste
    // until we ship our own menus.
    const onMenu = (e: MouseEvent) => {
      const t = e.target as HTMLElement | null;
      if (!t?.closest("input, textarea, [contenteditable]")) e.preventDefault();
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("contextmenu", onMenu);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("contextmenu", onMenu);
    };
  }, []);
}
