---
name: multi-account-mail-app
description: Project idea (Sept 2026) — free, ad-free native Windows multi-account mail client in Tauri with a "Sign in with Google" account-chooser feature
metadata:
  type: project
---

App is named **UniMail** (user chose the name). Lives at D:\dev\plume (folder still named plume). Not a git repo yet. Tauri 2 + React 19 + TS + Vite, plain CSS with tokens, lucide-react icons. Started 2026-09-10. Goal: a Tauri (Rust) Windows mail app with unlimited accounts (Gmail + any IMAP) all connected at once, instant sidebar switching, local cache, good design, free and ad-free.

Key wanted feature: intercept "Sign in with Google" flows (register as a browser and/or a "+" tab where you paste the auth URL), keep each Google account in its own isolated WebView2 profile, pick an account, then bounce the final callback URL to the real browser.

**Why:** user is frustrated by Gmail web slowness, cross-account sign-outs, and sharing friends' accounts.
**How to apply:** user got annoyed when I was too pessimistic about the sign-in interception; be concrete about what works (redirect-mode flows, desktop apps) vs what doesn't (popup/postMessage flows) instead of hedging.

Related: [[user-profile]]

**Backlog the user asked me to remember (2026-09-10):**
- OTP auto-copy: when an email arrives with a verification code, detect it and copy it to the clipboard automatically (like phone SMS autofill) so the user never has to open the app.
- User has "a different idea" for where the window close/minimize buttons should live; they'll explain later. Don't redesign that area until they do.
- User has never sent an email; compose is a low-priority icon, not a hero button.
- Right-click an account in the rail to pick its color; the color drives the account chip on rows and the avatar when the account has no image. Do this when real accounts are connected (2026-09-11).
- Bug to fix next run: dragging the window quickly to one side can close it (user reported 2026-09-11). Investigate Aero Snap / shake interaction with our custom drag.
- Font is Poppins, bundled locally in public/fonts. No settings screen: the user does not want users changing the design.

**State as of 2026-09-11 evening:** Gmail works via IMAP + app password (keyring). Per-account Google sessions run as separate helper processes (`unimail.exe --google <id> <email> [url]`, own WebView2 user-data folder) because WebView2 allows one data folder per process. Google-profile-picture fetching was tried and REMOVED at the user's request; accounts get a user-uploaded picture or a picked color via right-click. UniMail registers as a Windows browser (`links.rs`): Google login links -> account picker; all other links forwarded to the browser chosen in settings.json. Email HTML rendered as-designed inside a sandboxed iframe; Chromium auto-dark (`--enable-features=WebContentsForceDark`) darkens frames, app page declares `color-scheme: dark` to opt out. Decision: NO Google Cloud / OAuth; app password stays (user chose this after weighing it).

**Update 2026-09-12:** Chooser is its own frameless window (`?chooser=<url>`), one click per account, login_hint passed so Google skips its own picker. App built as `windows_subsystem = "windows"` in all builds (no console). Release build + MSI/NSIS installers exist; Windows browser registration points at target/release/unimail.exe. Open: Antigravity flow ended on "localhost refused to connect" once, unverified after fixes.
**User working style:** extremely detail-oriented about UI; gets angry at guessing. Change only exactly what's asked, verify the change actually reached the running app (Vite HMR has silently stalled before; restart dev server when unsure), never revert approved visuals unasked.

**Update 2026-10-10:** UniMail is INSTALLED for daily use at %LOCALAPPDATA%\UniMail\unimail.exe (NSIS per-user install, Start menu + desktop shortcuts); the user runs it themselves now. Link-handler registry entries point at the installed exe. Autostart is off (tray menu toggles it). Default browser is still Firefox-family, so Google sign-in interception only works via paste unless they set UniMail as default. Since 2026-10-06 the app also has IMAP IDLE push, OTP auto-copy (otp.rs, tested via a standalone crate in %TEMP%\otp_test), tray + close-to-tray, account error badge.
**How to apply:** the installed copy and a dev build share data and the single-instance lock. Before `npm run tauri dev`, quit the installed app from the tray, or the dev build just hands off and exits. After shipping changes the user should keep, rebuild (`npm run tauri build`) and rerun the NSIS setup with /S to update the install. Keep @tauri-apps/api and the tauri crate on the same minor version or the build refuses.
**Start menu gotcha (2026-10-16):** on this PC Windows search only indexes the ALL-USERS Start menu (C:\ProgramData\Microsoft\Windows\Start Menu); the per-user Start Menu is not in the index scope and the per-user app list wasn't picking up new shortcuts. UniMail's shortcut now lives in the ProgramData Start menu (copied with UAC), per-user one deleted. The user does NOT want a desktop shortcut. When reinstalling with the per-user NSIS installer, delete the per-user + desktop shortcuts it recreates; better long-term fix is NSIS installMode perMachine (would move the exe to Program Files: update link-handler registry paths then).
