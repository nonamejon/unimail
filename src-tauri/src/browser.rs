//! Per-account Google sessions.
//!
//! WebView2 allows only one user-data folder per process, so every account's Google window runs
//! in its own helper process (`unimail.exe --google <id> <email> [url]`) with its own cookie jar.
//! That gives each account a fully separate, persistent Google login, independent of every other
//! account and of the user's own browser.
//!
//! The helper does two things:
//!   1. Shows Google in that profile (app-passwords page by default, or a site's OAuth URL).
//!   2. Bounces the site's OAuth callback to the user's real browser and exits, so the site
//!      session lands where the user actually browses.

use crate::mail::account_id_for;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;

const APP_PASSWORDS_URL: &str = "https://myaccount.google.com/apppasswords";
const HELPER_WINDOW: &str = "google";

/// Hosts the Google login flow legitimately bounces through. Anything else is the site's callback.
fn is_google(host: &str) -> bool {
    const ALLOW: &[&str] = &[
        "google.com",
        "googleusercontent.com",
        "gstatic.com",
        "googleapis.com",
        "youtube.com",
        "ggpht.com",
        "googlevideo.com",
        "gvt1.com",
        "recaptcha.net",
    ];
    ALLOW.iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

/// Only a site's OAuth callback leaves the window: a non-Google http(s) URL carrying an OAuth
/// result (`code`/`state`/`error`), a loopback address (desktop apps), or a custom scheme.
/// Everything else Google wanders through during sign-in stays inside.
fn should_bounce(u: &Url) -> bool {
    let scheme = u.scheme();
    if scheme == "about" || scheme == "blob" || scheme == "data" {
        return false;
    }
    if scheme != "http" && scheme != "https" {
        return true; // custom scheme callback, e.g. com.example.app://oauth
    }
    let host = u.host_str().unwrap_or("");
    if host == "localhost" || host == "127.0.0.1" || host == "[::1]" {
        return true;
    }
    if is_google(host) {
        return false;
    }
    let q = u.query().unwrap_or("");
    let frag = u.fragment().unwrap_or("");
    let looks_oauth = |s: &str| s.split('&').any(|kv| kv.starts_with("code=") || kv.starts_with("state=") || kv.starts_with("error=") || kv.starts_with("access_token=") || kv.starts_with("id_token="));
    looks_oauth(q) || looks_oauth(frag)
}

fn log_nav(path: &std::path::Path, u: &Url, bounced: bool) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{} {} {}", chrono::Local::now().format("%H:%M:%S"), if bounced { "BOUNCE" } else { "allow " }, u);
    }
}

pub fn profile_dir(app: &AppHandle, account_id: &str) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("profiles")
        .join(account_id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

// ---------------------------------------------------------------- main-app side

/// Open a Google window for `email`'s profile at `url` (default: the app-passwords page).
#[tauri::command]
pub fn open_google(app: AppHandle, email: String, url: Option<String>) -> Result<(), String> {
    let email = email.trim().to_lowercase();
    let id = account_id_for(&email);
    let mut target = Url::parse(url.as_deref().unwrap_or(APP_PASSWORDS_URL)).map_err(|e| format!("Bad link: {e}"))?;
    // On an OAuth link, name the account so Google goes straight to consent instead of asking
    // "Choose an account" again inside a window that only has this one account anyway.
    if target.host_str() == Some("accounts.google.com") && target.path().contains("/o/oauth2") {
        let mut pairs: Vec<(String, String)> = target
            .query_pairs()
            .filter(|(k, _)| k != "login_hint" && k != "authuser")
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        pairs.push(("login_hint".into(), email.clone()));
        target.query_pairs_mut().clear().extend_pairs(pairs);
    }
    let target = target.to_string();
    profile_dir(&app, &id)?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(exe)
        .args(["--google", &id, &email, &target])
        .spawn()
        .map_err(|e| format!("Could not start the Google window: {e}"))?;
    Ok(())
}

/// Forget an account's Google session entirely.
pub fn remove_profile(app: &AppHandle, account_id: &str) {
    if let Ok(dir) = profile_dir(app, account_id) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

// ---------------------------------------------------------------- helper process

/// Entry point for `unimail.exe --google <id> <email> [url]`.
pub fn run_helper(mut ctx: tauri::Context<tauri::Wry>, args: Vec<String>) {
    let account_id = args.get(2).cloned().unwrap_or_default();
    let email = args.get(3).cloned().unwrap_or_default();
    let url = args.get(4).cloned().unwrap_or_else(|| APP_PASSWORDS_URL.to_string());
    // The helper must not create the main app window.
    ctx.config_mut().app.windows.clear();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let handle = app.handle().clone();
            let dir = profile_dir(&handle, &account_id)?;
            let target = Url::parse(&url)?;
            let bounce = handle.clone();
            let nav_log = dir.join("nav.log");
            let _ = &dir;

            WebviewWindowBuilder::new(app, HELPER_WINDOW, WebviewUrl::External(target))
                .title(format!("Google · {email}"))
                .inner_size(520.0, 780.0)
                .min_inner_size(400.0, 500.0)
                .center()
                .data_directory(dir.clone())
                // plain Chromium args here: no auto-dark for Google's own pages
                .additional_browser_args("--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection")
                .on_navigation(move |u| {
                    let bouncing = should_bounce(u);
                    log_nav(&nav_log, u, bouncing);
                    if !bouncing {
                        return true;
                    }
                    // The site's callback: deliver it to the real browser / waiting desktop app, then quit.
                    if crate::links::forward(&bounce, u.as_str()).is_err() {
                        let _ = bounce.opener().open_url(u.to_string(), None::<&str>);
                    }
                    let b = bounce.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(300));
                        b.exit(0);
                    });
                    false
                })
                .build()?;

            Ok(())
        })
        .run(ctx)
        .expect("error while running the Google window");
}
