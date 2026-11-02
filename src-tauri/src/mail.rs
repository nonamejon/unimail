//! Tauri commands for accounts and mail. The UI talks only to these.

use crate::{db, gmail};
use rusqlite::Connection;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

pub struct MailState {
    pub db: Arc<Mutex<Connection>>,
}

const KEYRING_SERVICE: &str = "UniMail";
const GMAIL_HOST: &str = "imap.gmail.com";
const GMAIL_PORT: u16 = 993;

pub(crate) fn account_id_for(email: &str) -> String {
    // Stable, filesystem/keyring safe id derived from the address.
    let mut h: u64 = 1469598103934665603;
    for b in email.trim().to_lowercase().bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    format!("acct_{h:016x}")
}

pub(crate) fn password_for(account_id: &str) -> Result<String, String> {
    keyring::Entry::new(KEYRING_SERVICE, account_id)
        .and_then(|e| e.get_password())
        .map_err(|e| format!("No saved password for this account: {e}"))
}

pub(crate) fn lock(db: &Arc<Mutex<Connection>>) -> std::sync::MutexGuard<'_, Connection> {
    db.lock().unwrap_or_else(|p| p.into_inner())
}

/// One sync at a time: the push watcher and the poll can fire together.
static SYNC_GATE: Mutex<()> = Mutex::new(());

/// Serialized sync of one account (used by the poll, the push watcher, and account setup).
pub(crate) fn sync_account_serial(db: &Arc<Mutex<Connection>>, account: &db::AccountRow) -> Result<usize, String> {
    let _gate = SYNC_GATE.lock().unwrap_or_else(|e| e.into_inner());
    sync_account_blocking(db, account)
}

/// Sync every folder we care about for one account. Blocking; run on a worker thread.
fn sync_account_blocking(db: &Arc<Mutex<Connection>>, account: &db::AccountRow) -> Result<usize, String> {
    let password = password_for(&account.id)?;
    let mut session = gmail::connect(&account.imap_host, account.imap_port, &account.email, &password)?;
    let folders = gmail::discover_folders(&mut session)?;

    let mut targets = vec![(folders.inbox.clone(), "inbox".to_string())];
    if let Some(sent) = &folders.sent {
        targets.push((sent.clone(), "sent".to_string()));
    }

    let mut added = 0;
    for (server_folder, our_folder) in targets {
        let prev = db::sync_state(&lock(db), &account.id, &our_folder).map_err(|e| e.to_string())?;
        let result = gmail::sync_folder(&mut session, &server_folder, &our_folder, prev)?;
        let fresh = !matches!(prev, Some((v, _)) if v == result.uid_validity);
        let mut conn = lock(db);
        if fresh {
            db::clear_folder(&conn, &account.id, &our_folder).map_err(|e| e.to_string())?;
        } else {
            db::retain_uids(&conn, &account.id, &our_folder, &result.live_uids).map_err(|e| e.to_string())?;
            db::update_flags(&mut conn, &account.id, &our_folder, &result.flags).map_err(|e| e.to_string())?;
        }
        added += result.new.len();
        db::upsert_messages(&mut conn, &account.id, &result.new).map_err(|e| e.to_string())?;
        db::set_sync_state(&conn, &account.id, &our_folder, result.uid_validity, result.last_uid).map_err(|e| e.to_string())?;
    }
    let _ = session.logout();
    Ok(added)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub account_id: String,
    pub added: usize,
    pub error: Option<String>,
}

// ---------------- commands ----------------

#[tauri::command]
pub async fn list_accounts(state: State<'_, MailState>) -> Result<Vec<db::Account>, String> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || db::list_accounts(&lock(&db)).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

/// Add a Gmail account with an app password. Verifies the login before saving anything.
#[tauri::command]
pub async fn add_gmail_account(
    app: AppHandle,
    state: State<'_, MailState>,
    name: String,
    email: String,
    app_password: String,
) -> Result<db::Account, String> {
    let db = state.db.clone();
    let email = email.trim().to_lowercase();
    let password: String = app_password.chars().filter(|c| !c.is_whitespace()).collect();
    if !email.contains('@') {
        return Err("That doesn't look like an email address.".into());
    }
    if password.len() != 16 {
        return Err("A Google app password is 16 letters. Copy it exactly as Google shows it.".into());
    }
    let name = if name.trim().is_empty() { email.split('@').next().unwrap_or("Account").to_string() } else { name.trim().to_string() };

    let account = tauri::async_runtime::spawn_blocking(move || -> Result<db::Account, String> {
        // 1. prove the credentials work
        let mut session = gmail::connect(GMAIL_HOST, GMAIL_PORT, &email, &password)?;
        let _ = session.logout();
        // 2. store the secret in the OS credential manager, never in our database
        let id = account_id_for(&email);
        keyring::Entry::new(KEYRING_SERVICE, &id)
            .and_then(|e| e.set_password(&password))
            .map_err(|e| format!("Could not save the password to Windows Credential Manager: {e}"))?;
        // 3. record the account
        {
            let conn = lock(&db);
            // Already connected: the new password simply replaces the old one (e.g. after a Google
            // password change killed the previous app password).
            if let Err(e) = db::insert_account(&conn, &id, &name, &email, "gmail", GMAIL_HOST, GMAIL_PORT) {
                if !e.to_string().contains("UNIQUE") {
                    return Err(e.to_string());
                }
            }
        }
        // 4. first sync
        let row = db::get_account(&lock(&db), &id).map_err(|e| e.to_string())?.ok_or("account vanished")?;
        sync_account_serial(&db, &row)?;
        let accounts = db::list_accounts(&lock(&db)).map_err(|e| e.to_string())?;
        accounts.into_iter().find(|a| a.id == id).ok_or_else(|| "account vanished".to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    let _ = app.emit("mail:updated", ());
    Ok(account)
}

#[tauri::command]
pub async fn remove_account(app: AppHandle, state: State<'_, MailState>, account_id: String) -> Result<(), String> {
    let db = state.db.clone();
    let app_for_profile = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        db::delete_account(&lock(&db), &account_id).map_err(|e| e.to_string())?;
        if let Ok(e) = keyring::Entry::new(KEYRING_SERVICE, &account_id) {
            let _ = e.delete_credential();
        }
        crate::browser::remove_profile(&app_for_profile, &account_id);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())??;
    let _ = app.emit("mail:updated", ());
    Ok(())
}

/// Sync all accounts (or one). Emits `mail:updated` when done. Errors are per account.
#[tauri::command]
pub async fn sync_mail(app: AppHandle, state: State<'_, MailState>, account_id: Option<String>) -> Result<Vec<SyncReport>, String> {
    let db = state.db.clone();
    let reports = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<SyncReport>, String> {
        let rows = match &account_id {
            Some(id) => db::get_account(&lock(&db), id).map_err(|e| e.to_string())?.into_iter().collect(),
            None => db::all_account_rows(&lock(&db)).map_err(|e| e.to_string())?,
        };
        let mut reports = Vec::new();
        for row in rows {
            let r = sync_account_serial(&db, &row);
            reports.push(SyncReport {
                account_id: row.id.clone(),
                added: *r.as_ref().unwrap_or(&0),
                error: r.err(),
            });
        }
        Ok(reports)
    })
    .await
    .map_err(|e| e.to_string())??;
    let _ = app.emit("mail:updated", ());
    let (app2, db2) = (app.clone(), state.db.clone());
    tauri::async_runtime::spawn_blocking(move || crate::push::check_codes(&app2, &db2));
    Ok(reports)
}

#[tauri::command]
pub async fn list_messages(state: State<'_, MailState>, account_id: Option<String>) -> Result<Vec<db::Message>, String> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        db::list_messages(&lock(&db), account_id.as_deref(), 600).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_sender_thread(state: State<'_, MailState>, account_id: String, from_email: String) -> Result<Vec<db::Message>, String> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || db::sender_messages(&lock(&db), &account_id, &from_email).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn rename_account(app: AppHandle, state: State<'_, MailState>, account_id: String, name: String) -> Result<(), String> {
    let db = state.db.clone();
    let name = name.trim().to_string();
    if name.is_empty() { return Err("Name can't be empty.".into()); }
    tauri::async_runtime::spawn_blocking(move || db::rename_account(&lock(&db), &account_id, &name).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    let _ = app.emit("mail:updated", ());
    Ok(())
}

#[tauri::command]
pub async fn set_account_hue(app: AppHandle, state: State<'_, MailState>, account_id: String, hue: String) -> Result<(), String> {
    const HUES: [&str; 8] = ["coral", "sun", "leaf", "sky", "grape", "mint", "rose", "slate"];
    if !HUES.contains(&hue.as_str()) { return Err("Unknown color.".into()); }
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || db::set_account_hue(&lock(&db), &account_id, &hue).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    let _ = app.emit("mail:updated", ());
    Ok(())
}

/// Read an image file the user picked and hand it to the UI as a data: URL (the UI resizes it).
#[tauri::command]
pub async fn read_image(path: String) -> Result<String, String> {
    use base64::Engine;
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    if bytes.len() > 25 * 1024 * 1024 { return Err("That image is too large.".into()); }
    let lower = path.to_lowercase();
    let mime = if lower.ends_with(".png") { "image/png" } else if lower.ends_with(".gif") { "image/gif" } else if lower.ends_with(".webp") { "image/webp" } else { "image/jpeg" };
    Ok(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(&bytes)))
}

/// Store (or clear, with null) an account's custom picture.
#[tauri::command]
pub async fn set_account_picture(app: AppHandle, state: State<'_, MailState>, account_id: String, data_url: Option<String>) -> Result<(), String> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = lock(&db);
        conn.execute("UPDATE accounts SET avatar = ?1 WHERE id = ?2", rusqlite::params![data_url, account_id]).map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| e.to_string())??;
    let _ = app.emit("mail:updated", ());
    Ok(())
}

#[tauri::command]
pub async fn get_thread(state: State<'_, MailState>, account_id: String, thread_id: String) -> Result<Vec<db::Message>, String> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || db::thread_messages(&lock(&db), &account_id, &thread_id).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

/// Applies a flag locally right away, then tells the server in the background.
fn flag_then_sync<F>(db: Arc<Mutex<Connection>>, id: i64, local: F, remote: impl FnOnce(&mut gmail::Session, &str, u32) -> Result<(), String> + Send + 'static)
where
    F: FnOnce(&Connection) -> rusqlite::Result<()> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let located = {
            let conn = lock(&db);
            let _ = local(&conn);
            db::locate(&conn, id).ok().flatten()
        };
        let Some((account_id, folder, uid)) = located else { return };
        let Ok(Some(account)) = db::get_account(&lock(&db), &account_id) else { return };
        let Ok(password) = password_for(&account.id) else { return };
        let Ok(mut session) = gmail::connect(&account.imap_host, account.imap_port, &account.email, &password) else { return };
        let server_folder = match folder.as_str() {
            "sent" => gmail::discover_folders(&mut session).ok().and_then(|f| f.sent).unwrap_or_else(|| "INBOX".into()),
            _ => "INBOX".into(),
        };
        let _ = remote(&mut session, &server_folder, uid);
        let _ = session.logout();
    });
}

#[tauri::command]
pub async fn mark_read(state: State<'_, MailState>, id: i64, read: bool) -> Result<(), String> {
    flag_then_sync(state.db.clone(), id, move |c| db::set_unread(c, id, !read), move |s, f, uid| gmail::set_seen(s, f, uid, read));
    Ok(())
}

#[tauri::command]
pub async fn set_starred(state: State<'_, MailState>, id: i64, starred: bool) -> Result<(), String> {
    flag_then_sync(state.db.clone(), id, move |c| db::set_starred(c, id, starred), move |s, f, uid| gmail::set_flagged(s, f, uid, starred));
    Ok(())
}

/// Archive = move out of the inbox into All Mail. Delete = move to Trash.
#[tauri::command]
pub async fn move_message(app: AppHandle, state: State<'_, MailState>, id: i64, to: String) -> Result<(), String> {
    let db = state.db.clone();
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let located = {
            let conn = lock(&db);
            let l = db::locate(&conn, id).ok().flatten();
            let _ = db::delete_message(&conn, id);
            l
        };
        let _ = app2.emit("mail:updated", ());
        let Some((account_id, folder, uid)) = located else { return };
        let Ok(Some(account)) = db::get_account(&lock(&db), &account_id) else { return };
        let Ok(password) = password_for(&account.id) else { return };
        let Ok(mut session) = gmail::connect(&account.imap_host, account.imap_port, &account.email, &password) else { return };
        let Ok(folders) = gmail::discover_folders(&mut session) else { return };
        let dest = match to.as_str() {
            "trash" => folders.trash,
            _ => folders.all,
        };
        let from = match folder.as_str() {
            "sent" => folders.sent.unwrap_or_else(|| "INBOX".into()),
            _ => "INBOX".into(),
        };
        if let Some(dest) = dest {
            let _ = gmail::move_to(&mut session, &from, uid, &dest);
        }
        let _ = session.logout();
    });
    Ok(())
}
