//! UniMail as a browser handler.
//!
//! Registered as a web browser on Windows, UniMail receives every link the system opens. A Google
//! sign-in link becomes an account picker in the app; every other link is forwarded, untouched, to
//! the browser the user actually uses. That's how "Sign in with Google" from any site or app lands
//! on UniMail's accounts instead of whatever the default browser happens to be logged into.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const PROG_ID: &str = "UniMailURL";
const CLIENT_KEY: &str = r"Software\Clients\StartMenuInternet\UniMail";

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct LinkSettings {
    /// Executable of the browser that gets every non-Google link.
    pub browser: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct BrowserInfo {
    pub name: String,
    pub path: String,
}

#[derive(Serialize)]
pub struct LinkStatus {
    pub browsers: Vec<BrowserInfo>,
    pub browser: Option<String>,
    pub registered: bool,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

pub fn load_settings(app: &AppHandle) -> LinkSettings {
    settings_path(app)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_settings(app: &AppHandle, s: &LinkSettings) -> Result<(), String> {
    let p = settings_path(app)?;
    std::fs::write(p, serde_json::to_string_pretty(s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Is this a Google sign-in link that should go through the account picker?
pub fn is_google_login(url: &str) -> bool {
    match tauri::Url::parse(url) {
        Ok(u) => matches!(u.host_str(), Some("accounts.google.com")),
        Err(_) => false,
    }
}

/// Send a link to the user's real browser. Never to ourselves.
pub fn forward(app: &AppHandle, url: &str) -> Result<(), String> {
    let s = load_settings(app);
    let browser = s
        .browser
        .or_else(|| installed_browsers().into_iter().next().map(|b| b.path))
        .ok_or("No browser is set for links. Pick one in the Sign in with Google sheet.")?;
    std::process::Command::new(&browser)
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not start {browser}: {e}"))
}

/// What to do with a link handed to UniMail: picker for Google logins, forward for everything else.
pub fn handle_link(app: &AppHandle, url: &str) {
    if is_google_login(url) {
        open_chooser(app, url);
    } else {
        let _ = forward(app, url);
    }
}

/// The account picker: a small frameless window on top of everything, one click to sign in.
/// Runs the same bundle as the app with `?chooser=<url>`.
pub fn open_chooser(app: &AppHandle, url: &str) {
    use tauri::{WebviewUrl, WebviewWindowBuilder};
    if let Some(w) = app.get_webview_window("chooser") {
        let _ = w.close();
    }
    let page = format!("index.html?chooser={}", urlencoding(url));
    let app2 = app.clone();
    // Window creation must happen on the main thread; this may be called from any.
    let _ = app.run_on_main_thread(move || {
        let _ = WebviewWindowBuilder::new(&app2, "chooser", WebviewUrl::App(page.into()))
            .title("Sign in with Google")
            .inner_size(420.0, 520.0)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .center()
            .focused(true)
            .build();
    });
}

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ---------------------------------------------------------------- registry

/// The executable Windows should launch for links and at login. A dev build depends on the Vite
/// server, so when running one, point at the release build next to it if it exists.
pub fn stable_exe() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if cfg!(debug_assertions) {
        if let Some(target) = exe.parent().and_then(|p| p.parent()) {
            let release = target.join("release").join(exe.file_name().unwrap_or_default());
            if release.exists() {
                return Ok(release.to_string_lossy().to_string());
            }
        }
    }
    Ok(exe.to_string_lossy().to_string())
}

#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

#[cfg(windows)]
pub fn autostart_enabled() -> bool {
    use winreg::enums::*;
    winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(RUN_KEY)
        .and_then(|k| k.get_value::<String, _>("UniMail"))
        .is_ok()
}

/// Start with Windows, in the tray (no window), so push and code copying work from login.
#[cfg(windows)]
pub fn set_autostart(on: bool) -> Result<(), String> {
    use winreg::enums::*;
    let (run, _) = winreg::RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY).map_err(|e| e.to_string())?;
    if on {
        run.set_value("UniMail", &format!("\"{}\" --minimized", stable_exe()?)).map_err(|e| e.to_string())
    } else {
        let _ = run.delete_value("UniMail");
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn autostart_enabled() -> bool { false }
#[cfg(not(windows))]
pub fn set_autostart(_on: bool) -> Result<(), String> { Ok(()) }

#[cfg(windows)]
pub fn installed_browsers() -> Vec<BrowserInfo> {
    use winreg::enums::*;
    use winreg::RegKey;
    let mut out = Vec::new();
    for root in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        let Ok(clients) = RegKey::predef(root).open_subkey(r"Software\Clients\StartMenuInternet") else { continue };
        for name in clients.enum_keys().flatten() {
            if name == "UniMail" { continue; }
            let Ok(k) = clients.open_subkey(&name) else { continue };
            let display: String = k.get_value("").unwrap_or(name.clone());
            let Ok(cmd) = k.open_subkey(r"shell\open\command") else { continue };
            let raw: String = cmd.get_value("").unwrap_or_default();
            let path = raw.trim().trim_matches('"').split('"').next().unwrap_or("").to_string();
            if path.is_empty() || out.iter().any(|b: &BrowserInfo| b.path.eq_ignore_ascii_case(&path)) { continue; }
            out.push(BrowserInfo { name: display, path });
        }
    }
    out
}

#[cfg(not(windows))]
pub fn installed_browsers() -> Vec<BrowserInfo> { Vec::new() }

/// Write the registry entries that make UniMail selectable as a web browser in Windows Settings.
#[cfg(windows)]
pub fn register() -> Result<(), String> {
    use winreg::enums::*;
    use winreg::RegKey;
    let exe = stable_exe()?;
    let open_cmd = format!("\"{exe}\" --open \"%1\"");
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let e = |x: std::io::Error| x.to_string();

    // ProgID for http/https
    let (prog, _) = hkcu.create_subkey(format!(r"Software\Classes\{PROG_ID}")).map_err(e)?;
    prog.set_value("", &"UniMail Link").map_err(e)?;
    prog.set_value("URL Protocol", &"").map_err(e)?;
    let (icon, _) = prog.create_subkey("DefaultIcon").map_err(e)?;
    icon.set_value("", &format!("\"{exe}\",0")).map_err(e)?;
    let (cmd, _) = prog.create_subkey(r"shell\open\command").map_err(e)?;
    cmd.set_value("", &open_cmd).map_err(e)?;

    // Browser client + capabilities
    let (client, _) = hkcu.create_subkey(CLIENT_KEY).map_err(e)?;
    client.set_value("", &"UniMail").map_err(e)?;
    let (ccmd, _) = client.create_subkey(r"shell\open\command").map_err(e)?;
    ccmd.set_value("", &format!("\"{exe}\"")).map_err(e)?;
    let (cicon, _) = client.create_subkey("DefaultIcon").map_err(e)?;
    cicon.set_value("", &format!("\"{exe}\",0")).map_err(e)?;
    let (caps, _) = client.create_subkey("Capabilities").map_err(e)?;
    caps.set_value("ApplicationName", &"UniMail").map_err(e)?;
    caps.set_value("ApplicationDescription", &"Sign in with Google through your UniMail accounts; every other link goes to your browser.").map_err(e)?;
    let (assoc, _) = caps.create_subkey("URLAssociations").map_err(e)?;
    assoc.set_value("http", &PROG_ID).map_err(e)?;
    assoc.set_value("https", &PROG_ID).map_err(e)?;
    let (reg, _) = hkcu.create_subkey(r"Software\RegisteredApplications").map_err(e)?;
    reg.set_value("UniMail", &format!(r"{CLIENT_KEY}\Capabilities")).map_err(e)?;
    Ok(())
}

#[cfg(not(windows))]
pub fn register() -> Result<(), String> { Err("Only implemented on Windows so far.".into()) }

#[cfg(windows)]
pub fn is_registered() -> bool {
    use winreg::enums::*;
    use winreg::RegKey;
    RegKey::predef(HKEY_CURRENT_USER).open_subkey(CLIENT_KEY).is_ok()
}
#[cfg(not(windows))]
pub fn is_registered() -> bool { false }

// ---------------------------------------------------------------- commands

#[tauri::command]
pub fn link_status(app: AppHandle) -> LinkStatus {
    let s = load_settings(&app);
    LinkStatus { browsers: installed_browsers(), browser: s.browser, registered: is_registered() }
}

#[tauri::command]
pub fn set_link_browser(app: AppHandle, path: String) -> Result<(), String> {
    let mut s = load_settings(&app);
    s.browser = Some(path);
    save_settings(&app, &s)
}

/// Register as a browser and open Windows' default-apps page so the user can pick UniMail.
#[tauri::command]
pub fn make_default_browser(app: AppHandle) -> Result<(), String> {
    register()?;
    // Windows 10: the default-apps page; the user picks UniMail under "Web browser".
    std::process::Command::new("cmd")
        .args(["/C", "start", "", "ms-settings:defaultapps"])
        .spawn()
        .map_err(|e| e.to_string())?;
    let _ = app;
    Ok(())
}

/// Frontend-triggered open in the real browser (used by the picker's own links).
#[tauri::command]
pub fn open_in_browser(app: AppHandle, url: String) -> Result<(), String> {
    forward(&app, &url)
}
