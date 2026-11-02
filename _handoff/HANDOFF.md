# UniMail: continue on another PC

Everything needed to pick this project up somewhere else.

## What's in this repo

- The whole app source (React UI in `src/`, Rust backend in `src-tauri/src/`).
- `_handoff/CHAT.md`: the full build conversation in readable form, from the first message.
- `_handoff/transcripts/`: the raw Claude Code session logs (`.jsonl`). Strings shaped like Google app passwords were redacted before upload.
- `_handoff/memory/`: Claude's project notes (decisions, user preferences, gotchas). Read `multi-account-mail-app.md` first.
- `_handoff/UniMail-original-icon.png`: the original U icon.

Not included on purpose: your mail database, saved app passwords (they live in Windows Credential Manager), and the per-account Google sessions. On a new PC you re-add accounts with fresh app passwords.

## Set up a new Windows PC

1. Install Node.js (LTS) and Git.
2. Install Rust with rustup. This project was built on the `stable-x86_64-pc-windows-gnu` toolchain with MinGW (scoop `mingw`). The MSVC toolchain also works if you delete `src-tauri/.cargo/config.toml`, which only exists to speed up linking on the GNU toolchain (and contains a path specific to the old PC).
3. WebView2 runtime is already part of Windows 10/11.
4. Clone and run:

```
git clone https://github.com/nonamejon/unimail.git
cd unimail
npm install
npm run tauri dev
```

5. Build an installer: `npm run tauri build`. Installers land in `src-tauri/target/release/bundle/` (NSIS setup exe and MSI).

## Continuing with Claude Code

Open the cloned folder in Claude Code and tell it to read `_handoff/HANDOFF.md`, `_handoff/memory/multi-account-mail-app.md` and skim `_handoff/CHAT.md`. To restore Claude's memory exactly, copy `_handoff/memory/*.md` into that project's Claude memory folder.

## Gotchas from the old PC

- Keep `@tauri-apps/api` / `@tauri-apps/cli` on the same minor version as the `tauri` Rust crate, or `tauri build` refuses.
- The installed app and a dev build share data and a single-instance lock. Quit the installed one from the tray before `npm run tauri dev`.
- On the old PC, Windows search only indexed the all-users Start menu (`C:\ProgramData\Microsoft\Windows\Start Menu\Programs`), so the shortcut had to be copied there. The user does not want a desktop shortcut.
- "Make default browser" writes registry keys pointing at the exe path; reinstalling to a new path means pressing it again.

## Status at handoff (2026-11-03)

Working: Gmail via IMAP + app password, multiple accounts, instant push (IMAP IDLE), verification-code auto-copy with notification, tray + close-to-tray, Start with Windows, per-sender conversations, search, star/archive/delete, HTML mail with Chromium auto-dark, Sign in with Google account chooser (UniMail as a browser handler).

Next on the list: reply/forward, attachments, fast-drag-to-edge closing the window, retest the Antigravity sign-in flow, other providers (Outlook, iCloud, generic IMAP).
