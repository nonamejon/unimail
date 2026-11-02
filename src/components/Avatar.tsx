import { useEffect, useState } from "react";
import type { Hue } from "../api";

/** Personal mailbox domains: no company icon to show, fall back to the initial. */
const GENERIC = new Set([
  "gmail.com", "googlemail.com", "outlook.com", "hotmail.com", "live.com", "msn.com", "yahoo.com", "ymail.com",
  "icloud.com", "me.com", "mac.com", "aol.com", "proton.me", "protonmail.com", "mail.ru", "yandex.ru", "yandex.com",
  "gmx.com", "gmx.de", "web.de", "zoho.com", "fastmail.com", "hey.com",
]);

/** Icon URL for a sender's domain, or null for personal mailboxes. */
export function senderIcon(email: string): string | null {
  const domain = email.split("@")[1]?.toLowerCase();
  if (!domain || GENERIC.has(domain)) return null;
  // strip common mail subdomains so "email.steampowered.com" resolves to the brand
  const parts = domain.split(".");
  const root = parts.length > 2 ? parts.slice(-2).join(".") : domain;
  return "https://icons.duckduckgo.com/ip3/" + root + ".ico";
}

interface Props {
  name: string;
  hue: Hue;
  size?: number;
  /** Optional image; falls back to the initial if it fails to load. */
  src?: string | null;
}

export function Avatar({ name, hue, size = 40, src }: Props) {
  const initial = name.trim().charAt(0).toUpperCase();
  const [failed, setFailed] = useState(false);
  useEffect(() => setFailed(false), [src]);
  const showImg = !!src && !failed;
  return (
    <div
      className={"avatar hue-" + hue}
      data-img={showImg}
      data-icon={showImg && !src!.startsWith("data:")}
      style={{ width: size, height: size, fontSize: size * 0.44 }}
      aria-hidden
    >
      {showImg ? <img src={src!} alt="" onError={() => setFailed(true)} draggable={false} /> : initial}
    </div>
  );
}
