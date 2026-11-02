//! IMAP access to a Gmail (or any IMAP) mailbox, plus MIME parsing into our message shape.

use crate::db::NewMessage;
use imap::types::{Fetch, Flag, NameAttribute};
use mailparse::{MailHeaderMap, ParsedMail};
use native_tls::TlsConnector;
use std::net::TcpStream;

pub type Session = imap::Session<native_tls::TlsStream<TcpStream>>;

/// How many of the newest messages to pull on the first sync of a folder.
const INITIAL_FETCH: usize = 150;

pub fn connect(host: &str, port: u16, email: &str, password: &str) -> Result<Session, String> {
    let tls = TlsConnector::builder().build().map_err(|e| e.to_string())?;
    let client = imap::connect((host, port), host, &tls).map_err(|e| format!("Could not reach {host}: {e}"))?;
    client
        .login(email, password)
        .map_err(|e| match e.0 {
            imap::Error::No(_) | imap::Error::Bad(_) => "Google rejected the sign-in. Check the email and the app password.".to_string(),
            other => other.to_string(),
        })
}

/// Server folder names for the roles we care about. Gmail localises "[Gmail]/Sent Mail",
/// so we look for the \Sent attribute instead of guessing the name.
pub struct Folders {
    pub inbox: String,
    pub sent: Option<String>,
    pub all: Option<String>,
    pub trash: Option<String>,
}

pub fn discover_folders(session: &mut Session) -> Result<Folders, String> {
    let names = session.list(None, Some("*")).map_err(|e| e.to_string())?;
    let mut f = Folders { inbox: "INBOX".into(), sent: None, all: None, trash: None };
    for n in names.iter() {
        for a in n.attributes() {
            if let NameAttribute::Custom(c) = a {
                match c.as_ref() {
                    "\\Sent" => f.sent = Some(n.name().to_string()),
                    "\\All" => f.all = Some(n.name().to_string()),
                    "\\Trash" => f.trash = Some(n.name().to_string()),
                    _ => {}
                }
            }
        }
    }
    Ok(f)
}

pub struct FolderSync {
    pub uid_validity: u32,
    /// Every UID currently in the folder (for pruning what was deleted elsewhere).
    pub live_uids: Vec<u32>,
    /// Flags for UIDs we already had.
    pub flags: Vec<(u32, bool, bool)>,
    /// Fully fetched new messages.
    pub new: Vec<NewMessage>,
    pub last_uid: u32,
}

/// Pull what changed in one folder since `last_uid`. `None` means first sync.
pub fn sync_folder(session: &mut Session, server_folder: &str, our_folder: &str, prev: Option<(u32, u32)>) -> Result<FolderSync, String> {
    let mailbox = session.select(server_folder).map_err(|e| e.to_string())?;
    let uid_validity = mailbox.uid_validity.unwrap_or(0);
    let fresh = match prev {
        Some((v, _)) if v == uid_validity => false,
        _ => true,
    };
    let last_uid = if fresh { 0 } else { prev.map(|p| p.1).unwrap_or(0) };

    let mut live: Vec<u32> = session.uid_search("ALL").map_err(|e| e.to_string())?.into_iter().collect();
    live.sort_unstable();

    // Which UIDs to fetch in full: everything newer than last_uid, capped on a first sync.
    let mut to_fetch: Vec<u32> = live.iter().copied().filter(|u| *u > last_uid).collect();
    if fresh && to_fetch.len() > INITIAL_FETCH {
        to_fetch = to_fetch[to_fetch.len() - INITIAL_FETCH..].to_vec();
    }
    // Flags for the ones we already have.
    let known: Vec<u32> = live.iter().copied().filter(|u| *u <= last_uid).collect();

    let mut flags = Vec::new();
    if !known.is_empty() {
        let set = known.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
        let fetched = session.uid_fetch(set, "(UID FLAGS)").map_err(|e| e.to_string())?;
        for f in fetched.iter() {
            if let Some(uid) = f.uid {
                let (unread, starred) = read_flags(f);
                flags.push((uid, unread, starred));
            }
        }
    }

    let mut new = Vec::new();
    if !to_fetch.is_empty() {
        // Fetch in chunks so one huge mailbox doesn't become one huge response.
        for chunk in to_fetch.chunks(40) {
            let set = chunk.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
            let fetched = session
                .uid_fetch(set, "(UID FLAGS INTERNALDATE BODY.PEEK[])")
                .map_err(|e| e.to_string())?;
            for f in fetched.iter() {
                if let Some(m) = to_message(f, our_folder) {
                    new.push(m);
                }
            }
        }
    }

    let newest = live.last().copied().unwrap_or(0);
    // If the first sync was capped, remember the oldest fetched UID so older mail is not
    // mistaken for "new" later; we simply never go further back than the cap for now.
    let last = if fresh && !to_fetch.is_empty() { newest } else { newest.max(last_uid) };

    Ok(FolderSync { uid_validity, live_uids: if fresh { to_fetch.clone() } else { live }, flags, new, last_uid: last })
}

fn read_flags(f: &Fetch) -> (bool, bool) {
    let mut seen = false;
    let mut flagged = false;
    for fl in f.flags() {
        match fl {
            Flag::Seen => seen = true,
            Flag::Flagged => flagged = true,
            _ => {}
        }
    }
    (!seen, flagged)
}

fn to_message(f: &Fetch, folder: &str) -> Option<NewMessage> {
    let uid = f.uid?;
    let raw = f.body()?;
    let parsed = mailparse::parse_mail(raw).ok()?;
    let (unread, starred) = read_flags(f);

    let subject = parsed.headers.get_first_value("Subject").unwrap_or_default();
    let subject = if subject.trim().is_empty() { "(no subject)".to_string() } else { subject.trim().to_string() };

    let (from_name, from_email) = parsed
        .headers
        .get_first_header("From")
        .and_then(|h| mailparse::addrparse_header(h).ok())
        .and_then(|list| list.into_inner().into_iter().next())
        .map(|a| match a {
            mailparse::MailAddr::Single(s) => (s.display_name.unwrap_or_default(), s.addr),
            mailparse::MailAddr::Group(g) => (g.group_name, String::new()),
        })
        .unwrap_or_default();
    let from_name = if from_name.trim().is_empty() {
        from_email.split('@').next().unwrap_or("").to_string()
    } else {
        from_name.trim().trim_matches('"').to_string()
    };

    let date = parsed
        .headers
        .get_first_value("Date")
        .and_then(|d| mailparse::dateparse(&d).ok())
        .map(|secs| secs * 1000)
        .or_else(|| f.internal_date().map(|d| d.timestamp_millis()))
        .unwrap_or(0);

    let thread_id = thread_key(&parsed, &subject);

    let mut text = String::new();
    let mut html = String::new();
    collect_bodies(&parsed, &mut text, &mut html);
    let html = if html.is_empty() { String::new() } else { sanitize_html(&html) };
    let snippet = snippet_of(if text.is_empty() { &html } else { &text });
    let text_body = if text.is_empty() && !html.is_empty() { strip_tags(&html) } else { text };

    Some(NewMessage {
        folder: folder.to_string(),
        uid,
        thread_id,
        subject,
        from_name,
        from_email,
        date,
        snippet,
        text_body,
        html_body: html,
        unread,
        starred,
    })
}

/// Group replies with their original: the first Message-ID in References is the root of the
/// conversation. Falls back to In-Reply-To, then the message's own id.
fn thread_key(mail: &ParsedMail, subject: &str) -> String {
    let refs = mail.headers.get_first_value("References").unwrap_or_default();
    if let Some(first) = refs.split_whitespace().next() {
        return first.to_string();
    }
    if let Some(irt) = mail.headers.get_first_value("In-Reply-To") {
        if let Some(first) = irt.split_whitespace().next() {
            return first.to_string();
        }
    }
    mail.headers
        .get_first_value("Message-ID")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("subject:{}", subject.to_lowercase()))
}

fn collect_bodies(part: &ParsedMail, text: &mut String, html: &mut String) {
    let mime = part.ctype.mimetype.to_lowercase();
    if mime.starts_with("multipart/") {
        for sub in &part.subparts {
            collect_bodies(sub, text, html);
        }
        return;
    }
    let is_attachment = part
        .get_content_disposition()
        .disposition == mailparse::DispositionType::Attachment;
    if is_attachment {
        return;
    }
    if mime == "text/plain" && text.is_empty() {
        if let Ok(b) = part.get_body() { *text = b; }
    } else if mime == "text/html" && html.is_empty() {
        if let Ok(b) = part.get_body() { *html = b; }
    }
}

/// Strip scripts, forms, frames and event handlers but keep the presentation: `<style>` blocks,
/// inline styles, and the table attributes marketing mail is built on. The viewer renders the
/// result in a script-free iframe with a strict Content-Security-Policy, so leftover CSS can't
/// run anything.
fn sanitize_html(html: &str) -> String {
    let mut b = ammonia::Builder::default();
    b.add_tags(&["style", "font", "center"]);
    // keep <style> contents; drop <title>/<head> contents entirely instead of leaving their text behind
    b.rm_clean_content_tags(&["style"]);
    b.add_clean_content_tags(&["title", "head"]);
    b.add_generic_attributes(&[
        "style", "class", "id", "width", "height", "align", "valign", "bgcolor", "background", "color",
        "border", "cellpadding", "cellspacing", "dir", "lang", "role", "face", "size", "nowrap",
    ]);
    b.add_tag_attributes("img", &["srcset", "sizes", "loading"]);
    b.add_url_schemes(&["cid", "data"]);
    b.clean(html).to_string()
}

fn snippet_of(s: &str) -> String {
    let plain = if s.contains('<') { strip_tags(s) } else { s.to_string() };
    let collapsed: String = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = collapsed.chars().take(140).collect();
    if collapsed.chars().count() > 140 { out.push('…'); }
    out
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut skip_depth = 0; // inside <style>/<script>
    let lower = html.to_lowercase();
    let bytes = lower.as_bytes();
    let mut i = 0;
    let chars: Vec<char> = html.chars().collect();
    // Simple state machine over chars; good enough for previews.
    let _ = (bytes, i);
    let mut idx = 0;
    while idx < chars.len() {
        let c = chars[idx];
        if c == '<' {
            in_tag = true;
            let rest: String = chars[idx..].iter().take(8).collect::<String>().to_lowercase();
            if rest.starts_with("<style") || rest.starts_with("<script") { skip_depth += 1; }
            if rest.starts_with("</style") || rest.starts_with("</scrip") { skip_depth = (skip_depth - 1).max(0); }
        } else if c == '>' {
            in_tag = false;
            out.push(' ');
        } else if !in_tag && skip_depth == 0 {
            out.push(c);
        }
        idx += 1;
        i = idx;
    }
    // decode the handful of entities that matter for previews
    out.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'")
}

// ---- server-side actions ----

pub fn set_seen(session: &mut Session, folder: &str, uid: u32, seen: bool) -> Result<(), String> {
    session.select(folder).map_err(|e| e.to_string())?;
    let op = if seen { "+FLAGS (\\Seen)" } else { "-FLAGS (\\Seen)" };
    session.uid_store(uid.to_string(), op).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn set_flagged(session: &mut Session, folder: &str, uid: u32, flagged: bool) -> Result<(), String> {
    session.select(folder).map_err(|e| e.to_string())?;
    let op = if flagged { "+FLAGS (\\Flagged)" } else { "-FLAGS (\\Flagged)" };
    session.uid_store(uid.to_string(), op).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn move_to(session: &mut Session, from_folder: &str, uid: u32, dest: &str) -> Result<(), String> {
    session.select(from_folder).map_err(|e| e.to_string())?;
    session.uid_mv(uid.to_string(), dest).map_err(|e| e.to_string())?;
    Ok(())
}
