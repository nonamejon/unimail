//! Local mail store. Everything the UI shows comes from here, never straight from the network.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub name: String,
    pub email: String,
    pub provider: String,
    pub hue: String,
    pub unread: i64,
    /// data: URL of the cached profile picture, if we have one.
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: i64,
    pub account_id: String,
    pub folder: String,
    pub uid: i64,
    pub thread_id: String,
    pub subject: String,
    pub from_name: String,
    pub from_email: String,
    /// Unix milliseconds.
    pub date: i64,
    pub snippet: String,
    pub text_body: String,
    pub html_body: String,
    pub unread: bool,
    pub starred: bool,
}

/// A message as it arrives from the sync, before it has a row id.
pub struct NewMessage {
    pub folder: String,
    pub uid: u32,
    pub thread_id: String,
    pub subject: String,
    pub from_name: String,
    pub from_email: String,
    pub date: i64,
    pub snippet: String,
    pub text_body: String,
    pub html_body: String,
    pub unread: bool,
    pub starred: bool,
}

const HUES: [&str; 8] = ["coral", "sky", "leaf", "sun", "grape", "mint", "rose", "slate"];

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS accounts (
           id TEXT PRIMARY KEY,
           name TEXT NOT NULL,
           email TEXT NOT NULL UNIQUE,
           provider TEXT NOT NULL,
           hue TEXT NOT NULL,
           imap_host TEXT NOT NULL,
           imap_port INTEGER NOT NULL,
           created_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS messages (
           id INTEGER PRIMARY KEY,
           account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
           folder TEXT NOT NULL,
           uid INTEGER NOT NULL,
           thread_id TEXT NOT NULL,
           subject TEXT NOT NULL,
           from_name TEXT NOT NULL,
           from_email TEXT NOT NULL,
           date INTEGER NOT NULL,
           snippet TEXT NOT NULL,
           text_body TEXT NOT NULL,
           html_body TEXT NOT NULL,
           unread INTEGER NOT NULL,
           starred INTEGER NOT NULL,
           UNIQUE(account_id, folder, uid)
         );
         CREATE INDEX IF NOT EXISTS messages_by_account_date ON messages(account_id, date DESC);
         CREATE INDEX IF NOT EXISTS messages_by_thread ON messages(account_id, thread_id);
         CREATE TABLE IF NOT EXISTS sync_state (
           account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
           folder TEXT NOT NULL,
           uid_validity INTEGER NOT NULL,
           last_uid INTEGER NOT NULL,
           PRIMARY KEY(account_id, folder)
         );",
    )?;
    // added later; ignore "duplicate column" on databases that already have it
    let _ = conn.execute("ALTER TABLE accounts ADD COLUMN avatar TEXT", []);
    Ok(conn)
}

pub fn list_accounts(conn: &Connection) -> rusqlite::Result<Vec<Account>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.name, a.email, a.provider, a.hue,
                (SELECT COUNT(*) FROM messages m WHERE m.account_id = a.id AND m.folder = 'inbox' AND m.unread = 1),
                a.avatar
         FROM accounts a ORDER BY a.created_at",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Account {
            id: r.get(0)?,
            name: r.get(1)?,
            email: r.get(2)?,
            provider: r.get(3)?,
            hue: r.get(4)?,
            unread: r.get(5)?,
            avatar: r.get(6)?,
        })
    })?;
    rows.collect()
}

pub struct AccountRow {
    pub id: String,
    pub email: String,
    pub imap_host: String,
    pub imap_port: u16,
}

pub fn get_account(conn: &Connection, id: &str) -> rusqlite::Result<Option<AccountRow>> {
    conn.query_row(
        "SELECT id, email, imap_host, imap_port FROM accounts WHERE id = ?1",
        params![id],
        |r| {
            Ok(AccountRow {
                id: r.get(0)?,
                email: r.get(1)?,
                imap_host: r.get(2)?,
                imap_port: r.get::<_, i64>(3)? as u16,
            })
        },
    )
    .optional()
}

pub fn all_account_rows(conn: &Connection) -> rusqlite::Result<Vec<AccountRow>> {
    let mut stmt = conn.prepare("SELECT id, email, imap_host, imap_port FROM accounts ORDER BY created_at")?;
    let rows = stmt.query_map([], |r| {
        Ok(AccountRow {
            id: r.get(0)?,
            email: r.get(1)?,
            imap_host: r.get(2)?,
            imap_port: r.get::<_, i64>(3)? as u16,
        })
    })?;
    rows.collect()
}

pub fn insert_account(
    conn: &Connection,
    id: &str,
    name: &str,
    email: &str,
    provider: &str,
    imap_host: &str,
    imap_port: u16,
) -> rusqlite::Result<()> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))?;
    let hue = HUES[(count as usize) % HUES.len()];
    conn.execute(
        "INSERT INTO accounts (id, name, email, provider, hue, imap_host, imap_port, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, name, email, provider, hue, imap_host, imap_port as i64, chrono::Utc::now().timestamp_millis()],
    )?;
    Ok(())
}

pub fn rename_account(conn: &Connection, id: &str, name: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE accounts SET name = ?1 WHERE id = ?2", params![name, id])?;
    Ok(())
}

pub fn set_account_hue(conn: &Connection, id: &str, hue: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE accounts SET hue = ?1 WHERE id = ?2", params![hue, id])?;
    Ok(())
}

pub fn delete_account(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn sync_state(conn: &Connection, account_id: &str, folder: &str) -> rusqlite::Result<Option<(u32, u32)>> {
    conn.query_row(
        "SELECT uid_validity, last_uid FROM sync_state WHERE account_id = ?1 AND folder = ?2",
        params![account_id, folder],
        |r| Ok((r.get::<_, i64>(0)? as u32, r.get::<_, i64>(1)? as u32)),
    )
    .optional()
}

pub fn set_sync_state(conn: &Connection, account_id: &str, folder: &str, uid_validity: u32, last_uid: u32) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO sync_state (account_id, folder, uid_validity, last_uid) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(account_id, folder) DO UPDATE SET uid_validity = excluded.uid_validity, last_uid = excluded.last_uid",
        params![account_id, folder, uid_validity as i64, last_uid as i64],
    )?;
    Ok(())
}

/// Forget everything cached for one folder (used when the server's UIDVALIDITY changes).
pub fn clear_folder(conn: &Connection, account_id: &str, folder: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM messages WHERE account_id = ?1 AND folder = ?2", params![account_id, folder])?;
    Ok(())
}

pub fn upsert_messages(conn: &mut Connection, account_id: &str, msgs: &[NewMessage]) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO messages (account_id, folder, uid, thread_id, subject, from_name, from_email, date,
                                   snippet, text_body, html_body, unread, starred)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(account_id, folder, uid) DO UPDATE SET unread = excluded.unread, starred = excluded.starred",
        )?;
        for m in msgs {
            stmt.execute(params![
                account_id, m.folder, m.uid as i64, m.thread_id, m.subject, m.from_name, m.from_email, m.date,
                m.snippet, m.text_body, m.html_body, m.unread as i64, m.starred as i64
            ])?;
        }
    }
    tx.commit()
}

/// Refresh only the flags of messages we already have (read / starred can change on other devices).
pub fn update_flags(conn: &mut Connection, account_id: &str, folder: &str, flags: &[(u32, bool, bool)]) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare("UPDATE messages SET unread = ?1, starred = ?2 WHERE account_id = ?3 AND folder = ?4 AND uid = ?5")?;
        for (uid, unread, starred) in flags {
            stmt.execute(params![*unread as i64, *starred as i64, account_id, folder, *uid as i64])?;
        }
    }
    tx.commit()
}

/// Drop cached messages that no longer exist on the server (moved, deleted elsewhere).
pub fn retain_uids(conn: &Connection, account_id: &str, folder: &str, live: &[u32]) -> rusqlite::Result<()> {
    let list = live.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
    let sql = if list.is_empty() {
        "DELETE FROM messages WHERE account_id = ?1 AND folder = ?2".to_string()
    } else {
        format!("DELETE FROM messages WHERE account_id = ?1 AND folder = ?2 AND uid NOT IN ({list})")
    };
    conn.execute(&sql, params![account_id, folder])?;
    Ok(())
}

fn row_to_message(r: &rusqlite::Row) -> rusqlite::Result<Message> {
    Ok(Message {
        id: r.get(0)?,
        account_id: r.get(1)?,
        folder: r.get(2)?,
        uid: r.get(3)?,
        thread_id: r.get(4)?,
        subject: r.get(5)?,
        from_name: r.get(6)?,
        from_email: r.get(7)?,
        date: r.get(8)?,
        snippet: r.get(9)?,
        text_body: r.get(10)?,
        html_body: r.get(11)?,
        unread: r.get::<_, i64>(12)? != 0,
        starred: r.get::<_, i64>(13)? != 0,
    })
}

const COLS: &str = "id, account_id, folder, uid, thread_id, subject, from_name, from_email, date, snippet, text_body, html_body, unread, starred";

/// Messages for the list. Bodies are left empty here to keep the payload small.
pub fn list_messages(conn: &Connection, account_id: Option<&str>, limit: i64) -> rusqlite::Result<Vec<Message>> {
    let cols = COLS.replace("text_body, html_body", "'' AS text_body, '' AS html_body");
    let mut out = Vec::new();
    match account_id {
        Some(id) => {
            let mut stmt = conn.prepare(&format!("SELECT {cols} FROM messages WHERE account_id = ?1 ORDER BY date DESC LIMIT ?2"))?;
            let rows = stmt.query_map(params![id, limit], row_to_message)?;
            for r in rows { out.push(r?); }
        }
        None => {
            let mut stmt = conn.prepare(&format!("SELECT {cols} FROM messages ORDER BY date DESC LIMIT ?1"))?;
            let rows = stmt.query_map(params![limit], row_to_message)?;
            for r in rows { out.push(r?); }
        }
    }
    Ok(out)
}

pub fn thread_messages(conn: &Connection, account_id: &str, thread_id: &str) -> rusqlite::Result<Vec<Message>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM messages WHERE account_id = ?1 AND thread_id = ?2 ORDER BY date DESC"
    ))?;
    let rows = stmt.query_map(params![account_id, thread_id], row_to_message)?;
    rows.collect()
}

/// Everything one sender has sent us in this account (the "chat with a person" view).
pub fn sender_messages(conn: &Connection, account_id: &str, from_email: &str) -> rusqlite::Result<Vec<Message>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM messages WHERE account_id = ?1 AND folder = 'inbox' AND from_email = ?2 COLLATE NOCASE ORDER BY date DESC"
    ))?;
    let rows = stmt.query_map(params![account_id, from_email], row_to_message)?;
    rows.collect()
}

pub fn set_unread(conn: &Connection, id: i64, unread: bool) -> rusqlite::Result<()> {
    conn.execute("UPDATE messages SET unread = ?1 WHERE id = ?2", params![unread as i64, id])?;
    Ok(())
}

pub fn set_starred(conn: &Connection, id: i64, starred: bool) -> rusqlite::Result<()> {
    conn.execute("UPDATE messages SET starred = ?1 WHERE id = ?2", params![starred as i64, id])?;
    Ok(())
}

pub fn delete_message(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM messages WHERE id = ?1", params![id])?;
    Ok(())
}

/// (account_id, folder, uid) for a message row, needed to talk to the server about it.
pub fn locate(conn: &Connection, id: i64) -> rusqlite::Result<Option<(String, String, u32)>> {
    conn.query_row(
        "SELECT account_id, folder, uid FROM messages WHERE id = ?1",
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? as u32)),
    )
    .optional()
}
