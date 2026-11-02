/** Small brand marks for mail providers. Inline SVG so they stay crisp and need no assets. */
export function ProviderMark({ id, size = 22 }: { id: "gmail" | "outlook" | "icloud" | "imap"; size?: number }) {
  const s = { width: size, height: size, flexShrink: 0 } as const;
  switch (id) {
    case "gmail":
      return (
        <svg viewBox="0 0 24 24" style={s} aria-hidden>
          <path d="M3 6.5v11A1.5 1.5 0 0 0 4.5 19H7V10.3L12 14l5-3.7V19h2.5a1.5 1.5 0 0 0 1.5-1.5v-11" fill="none" />
          <path d="M3 7.5V17.5A1.5 1.5 0 0 0 4.5 19H7V10.5L3 7.5z" fill="#4285F4" />
          <path d="M21 7.5V17.5a1.5 1.5 0 0 1-1.5 1.5H17V10.5l4-3z" fill="#34A853" />
          <path d="M7 10.5 3 7.5V6.6C3 5.2 4.6 4.4 5.7 5.3L7 6.3v4.2z" fill="#C5221F" />
          <path d="M17 10.5l4-3V6.6c0-1.4-1.6-2.2-2.7-1.3L17 6.3v4.2z" fill="#FBBC04" />
          <path d="M7 6.3 12 10l5-3.7v4.2L12 14 7 10.5V6.3z" fill="#EA4335" />
        </svg>
      );
    case "outlook":
      return (
        <svg viewBox="0 0 24 24" style={s} aria-hidden>
          <rect x="9" y="5" width="12" height="14" rx="1.5" fill="#1B66C9" />
          <path d="M9 9.5 15 13l6-3.5V8l-6 3.5L9 8v1.5z" fill="#0F4A9C" />
          <rect x="2" y="7" width="11" height="10" rx="1.5" fill="#0078D4" />
          <ellipse cx="7.5" cy="12" rx="2.6" ry="3.1" fill="none" stroke="#fff" strokeWidth="1.5" />
        </svg>
      );
    case "icloud":
      return (
        <svg viewBox="0 0 24 24" style={s} aria-hidden>
          <path d="M7 18h10.5a3.5 3.5 0 0 0 .4-6.98A5.5 5.5 0 0 0 7.3 9.5 4.25 4.25 0 0 0 7 18z" fill="#3B9CFF" />
          <path d="M7 18h10.5a3.5 3.5 0 0 0 .4-6.98A5.5 5.5 0 0 0 7.3 9.5 4.25 4.25 0 0 0 7 18z" fill="url(#ic)" />
          <defs><linearGradient id="ic" x1="4" y1="18" x2="20" y2="8"><stop stopColor="#2F80ED" /><stop offset="1" stopColor="#8AC4FF" /></linearGradient></defs>
        </svg>
      );
    default:
      return (
        <svg viewBox="0 0 24 24" style={s} aria-hidden fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
          <rect x="3" y="5" width="18" height="14" rx="2.5" />
          <path d="m4 8 8 5 8-5" />
        </svg>
      );
  }
}
