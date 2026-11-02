mod browser;
mod db;
mod gmail;
mod links;
mod mail;
mod otp;
mod push;

use std::sync::{Arc, Mutex};
use tauri::Window;

/// Turn the desktop compositor's restore/minimize animation on or off for this window.
/// While a user is dragging a maximized window we want it to snap to size instantly
/// so the OS drag can track the pointer without a visible pause.
#[cfg(windows)]
fn set_dwm_transitions(window: &Window, enabled: bool) {
    use windows::core::BOOL;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_TRANSITIONS_FORCEDISABLED};
    if let Ok(hwnd) = window.hwnd() {
        let disabled = BOOL::from(!enabled);
        unsafe {
            let _ = DwmSetWindowAttribute(
                HWND(hwnd.0),
                DWMWA_TRANSITIONS_FORCEDISABLED,
                &disabled as *const BOOL as *const _,
                std::mem::size_of::<BOOL>() as u32,
            );
        }
    }
}

/// Restore the maximized window straight into a rectangle positioned under the cursor.
/// One `SetWindowPlacement` call does both the un-maximize and the move, so the window
/// paints exactly once, already in the right place. No flicker, no catch-up jump.
#[cfg(windows)]
fn restore_under_cursor(window: &Window, fx: f64, grab_y: f64) -> Result<(), String> {
    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetWindowPlacement, SetWindowPlacement, SystemParametersInfoW,
        SPI_GETWORKAREA, SW_SHOWNORMAL, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WINDOWPLACEMENT,
    };
    let e = |err: windows::core::Error| err.to_string();
    let hwnd = HWND(window.hwnd().map_err(|x| x.to_string())?.0);
    unsafe {
        let mut wp = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
        GetWindowPlacement(hwnd, &mut wp).map_err(e)?;
        let w = wp.rcNormalPosition.right - wp.rcNormalPosition.left;
        let h = wp.rcNormalPosition.bottom - wp.rcNormalPosition.top;

        let mut cursor = POINT::default();
        GetCursorPos(&mut cursor).map_err(e)?;

        // rcNormalPosition is in workspace coordinates (excludes the taskbar area).
        let mut work = RECT::default();
        SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work as *mut RECT as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)).map_err(e)?;

        let x = (cursor.x as f64 - fx * w as f64).round() as i32 - work.left;
        let y = (cursor.y as f64 - grab_y.min(h as f64 * 0.5)).round() as i32 - work.top;
        wp.rcNormalPosition = RECT { left: x, top: y, right: x + w, bottom: y + h };
        wp.showCmd = SW_SHOWNORMAL.0 as u32;
        SetWindowPlacement(hwnd, &wp).map_err(e)?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_dwm_transitions(_window: &Window, _enabled: bool) {}

#[cfg(not(windows))]
fn restore_under_cursor(window: &Window, fx: f64, grab_y: f64) -> Result<(), String> {
    let err = |e: tauri::Error| e.to_string();
    window.unmaximize().map_err(err)?;
    let size = window.outer_size().map_err(err)?;
    let cursor = window.cursor_position().map_err(err)?;
    let x = cursor.x - fx * size.width as f64;
    let y = cursor.y - grab_y.min(size.height as f64 * 0.5);
    window
        .set_position(tauri::PhysicalPosition::new(x.round() as i32, y.round() as i32))
        .map_err(err)
}

/// Un-maximize the window under the cursor and hand it to the OS move loop, all in one
/// step on the window thread so the pointer can't drift in between.
///
/// `fx` is the horizontal grab point as a fraction of the maximized width (0..1);
/// `grab_y` is the grab point's distance from the window top in physical pixels.
#[tauri::command]
fn restore_and_drag(window: Window, fx: f64, grab_y: f64) -> Result<(), String> {
    restore_under_cursor(&window, fx, grab_y)?;
    window.start_dragging().map_err(|e| e.to_string())
}

/// Maximize/restore with the compositor's zoom animation, which is otherwise kept off
/// so the drag-to-restore gesture stays instant.
#[tauri::command]
fn toggle_maximize_animated(window: Window) -> Result<(), String> {
    set_dwm_transitions(&window, true);
    let r = if window.is_maximized().map_err(|e| e.to_string())? {
        window.unmaximize()
    } else {
        window.maximize()
    };
    let w = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(450));
        set_dwm_transitions(&w, false);
    });
    r.map_err(|e| e.to_string())
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = tauri::Manager::get_webview_window(app, "main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Tray icon: left click opens the window; menu has Open, Start with Windows, Quit.
fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    let open = MenuItem::with_id(app, "open", "Open UniMail", true, None::<&str>)?;
    let auto = CheckMenuItem::with_id(app, "autostart", "Start with Windows", true, links::autostart_enabled(), None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit UniMail", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &auto, &sep, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("UniMail")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "open" => show_main(app),
            "autostart" => { let _ = links::set_autostart(!links::autostart_enabled()); }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// `unimail.exe --open <url>` (from the browser registration) or a bare URL argument.
fn link_arg(argv: &[String]) -> Option<String> {
    let mut it = argv.iter().skip(1);
    while let Some(a) = it.next() {
        if a == "--open" { return it.next().cloned(); }
        if a.starts_with("http://") || a.starts_with("https://") { return Some(a.clone()); }
    }
    None
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let ctx = tauri::generate_context!();
    let args: Vec<String> = std::env::args().collect();
    // `unimail.exe --google <id> <email> [url]`: a Google window in its own process/profile.
    if args.get(1).map(|s| s.as_str()) == Some("--google") {
        return browser::run_helper(ctx, args);
    }

    tauri::Builder::default()
        // One running app: a second launch (e.g. Windows opening a link) hands its arguments here.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(url) = link_arg(&argv) {
                links::handle_link(app, &url);
            } else if !argv.iter().any(|a| a == "--minimized") {
                show_main(app);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        // Closing the window keeps UniMail in the tray so push and code copying keep working.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            use tauri::Manager;
            // Frameless windows draw their own chrome; the compositor's restore animation
            // only gets in the way of the drag-to-restore gesture. Off for good.
            if let Some(w) = app.get_webview_window("main") {
                set_dwm_transitions(&w.as_ref().window(), false);
            }
            // Local mail store lives in the per-user app data folder.
            let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
            std::fs::create_dir_all(&dir)?;
            let conn = db::open(&dir.join("mail.sqlite"))?;
            let db = Arc::new(Mutex::new(conn));
            app.manage(mail::MailState { db: db.clone() });
            push::start(app.handle().clone(), db);
            build_tray(app)?;
            if !std::env::args().any(|a| a == "--minimized") {
                show_main(app.handle());
            }
            // Launched by Windows with a link while not running: handle it once the UI is up.
            if let Some(url) = link_arg(&std::env::args().collect::<Vec<_>>()) {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1200));
                    links::handle_link(&h, &url);
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            restore_and_drag,
            toggle_maximize_animated,
            mail::list_accounts,
            mail::add_gmail_account,
            mail::remove_account,
            mail::sync_mail,
            mail::list_messages,
            mail::get_thread,
            mail::get_sender_thread,
            mail::rename_account,
            mail::set_account_hue,
            mail::mark_read,
            mail::set_starred,
            mail::move_message,
            browser::open_google,
            mail::read_image,
            mail::set_account_picture,
            links::link_status,
            links::set_link_browser,
            links::make_default_browser,
            links::open_in_browser
        ])
        .run(ctx)
        .expect("error while running tauri application");
}
