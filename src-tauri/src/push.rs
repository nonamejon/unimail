//! Instant mail and verification-code auto-copy.
//!
//! Each account keeps one extra IMAP connection parked in IDLE on its inbox. Gmail pushes a
//! notice the moment a message lands; we sync that account right away, then look through the
//! newly arrived mail for a one-time code. If one is found it goes straight to the clipboard and
//! a Windows notification says so, so the user can paste it without ever opening the app.

use crate::{db, gmail, mail};
use imap::extensions::idle::WaitOutcome;
use rusqlite::Connection;
use std::collections::HashSet;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

/// Highest message row id already checked for codes. Mail that was here before the app
/// started is never scanned, so old codes don't get copied on launch.
static SEEN_MAX: AtomicI64 = AtomicI64::new(i64::MAX);

/// Codes are only copied from mail that arrived recently.
const CODE_MAX_AGE_MS: i64 = 15 * 60 * 1000;
/// IDLE is re-issued this often; doubles as a keepalive and an "account removed?" check.
const IDLE_SLICE: Duration = Duration::from_secs(300);

pub fn start(app: AppHandle, dbh: Arc<Mutex<Connection>>) {
    {
        let conn = mail::lock(&dbh);
        let max: i64 = conn.query_row("SELECT COALESCE(MAX(id), 0) FROM messages", [], |r| r.get(0)).unwrap_or(0);
        SEEN_MAX.store(max, Ordering::SeqCst);
    }
    let running: Arc<Mutex<HashSet<String>>> = Arc::default();
    // Supervisor: start a watcher for every account that doesn't have one (new accounts included).
    std::thread::spawn(move || loop {
        let rows = db::all_account_rows(&mail::lock(&dbh)).unwrap_or_default();
        for row in rows {
            let fresh = running.lock().unwrap_or_else(|e| e.into_inner()).insert(row.id.clone());
            if fresh {
                let (app, dbh, running) = (app.clone(), dbh.clone(), running.clone());
                std::thread::spawn(move || {
                    watch(&app, &dbh, &row.id);
                    running.lock().unwrap_or_else(|e| e.into_inner()).remove(&row.id);
                });
            }
        }
        std::thread::sleep(Duration::from_secs(15));
    });
}

/// Keep an IDLE session alive for one account until the account is removed.
fn watch(app: &AppHandle, dbh: &Arc<Mutex<Connection>>, account_id: &str) {
    let mut backoff = Duration::from_secs(5);
    loop {
        let started = Instant::now();
        match idle_session(app, dbh, account_id) {
            Ok(()) => return, // account was removed
            Err(_) => {
                if started.elapsed() > Duration::from_secs(60) {
                    backoff = Duration::from_secs(5);
                }
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_secs(300));
            }
        }
    }
}

fn idle_session(app: &AppHandle, dbh: &Arc<Mutex<Connection>>, account_id: &str) -> Result<(), String> {
    let Some(acc) = db::get_account(&mail::lock(dbh), account_id).map_err(|e| e.to_string())? else {
        return Ok(());
    };
    let password = mail::password_for(&acc.id)?;
    let mut session = gmail::connect(&acc.imap_host, acc.imap_port, &acc.email, &password)?;
    session.select("INBOX").map_err(|e| e.to_string())?;
    // Catch up on anything that arrived while we weren't listening.
    sync_and_check(app, dbh, &acc);
    loop {
        if db::get_account(&mail::lock(dbh), account_id).map_err(|e| e.to_string())?.is_none() {
            let _ = session.logout();
            return Ok(());
        }
        let outcome = session
            .idle()
            .map_err(|e| e.to_string())?
            .wait_with_timeout(IDLE_SLICE)
            .map_err(|e| e.to_string())?;
        if matches!(outcome, WaitOutcome::MailboxChanged) {
            sync_and_check(app, dbh, &acc);
        }
    }
}

fn sync_and_check(app: &AppHandle, dbh: &Arc<Mutex<Connection>>, acc: &db::AccountRow) {
    if let Ok(added) = mail::sync_account_serial(dbh, acc) {
        if added > 0 {
            let _ = app.emit("mail:updated", ());
        }
    }
    check_codes(app, dbh);
}

/// Look through mail that arrived since the last check for a one-time code.
pub fn check_codes(app: &AppHandle, dbh: &Arc<Mutex<Connection>>) {
    let since = SEEN_MAX.load(Ordering::SeqCst);
    if since == i64::MAX {
        return; // not started yet
    }
    let rows: Vec<(i64, String, String, i64, String)> = {
        let conn = mail::lock(dbh);
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, subject, substr(text_body, 1, 6000), date, from_name FROM messages
             WHERE id > ?1 AND folder = 'inbox' ORDER BY date DESC",
        ) else { return };
        let mapped = stmt.query_map([since], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)));
        match mapped {
            Ok(it) => it.flatten().collect(),
            Err(_) => return,
        }
    };
    if let Some(max) = rows.iter().map(|r| r.0).max() {
        SEEN_MAX.fetch_max(max, Ordering::SeqCst);
    }
    let cutoff = chrono::Utc::now().timestamp_millis() - CODE_MAX_AGE_MS;
    for (_, subject, body, date, from) in rows {
        if date < cutoff {
            continue;
        }
        if let Some(code) = crate::otp::find_code(&subject, &body) {
            let copied = arboard::Clipboard::new().and_then(|mut c| c.set_text(code.clone())).is_ok();
            let title = if copied { format!("{code} copied") } else { format!("Code {code}") };
            let _ = app.notification().builder().title(title).body(format!("Verification code from {from}")).show();
            let _ = app.emit("otp:copied", code);
            break; // only the newest code
        }
    }
}
