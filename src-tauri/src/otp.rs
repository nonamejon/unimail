//! One-time code detection in email text. No app dependencies, so it can be tested on its own.

use regex::Regex;
use std::sync::OnceLock;

const KEYWORDS: &[&str] = &[
    "code", "verification", "verify", "otp", "one-time", "one time", "passcode", "pin", "security code",
    "sign in", "sign-in", "login", "log in", "confirm", "authentication", "2fa", "two-factor", "2-step",
    // ru / uz / tr / es / de / pt / fr
    "код", "подтвержд", "пароль", "kod", "tasdiq", "doğrulama", "código", "verificación", "bestätigung",
    "verifizierung", "confirmação", "vérification",
];

fn keyword_positions(lower: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for kw in KEYWORDS {
        let mut start = 0;
        while let Some(i) = lower[start..].find(kw) {
            let pos = start + i;
            let before_ok = lower[..pos].chars().next_back().map_or(true, |c| !c.is_alphanumeric());
            let after = lower[pos + kw.len()..].chars().next();
            let after_ok = kw.chars().count() > 4 || after.map_or(true, |c| !c.is_alphanumeric() || c == 's');
            if before_ok && after_ok {
                out.push(pos);
            }
            start = pos + kw.len();
        }
    }
    out
}

fn digits_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(?:[A-Z]{1,3}-)?(\d{3}[ -]\d{3}|\d{4,8})\b").unwrap())
}

/// Best code candidate in one piece of text, with a score (lower is better).
fn best_in(text: &str) -> Option<(usize, String)> {
    let lower = text.to_lowercase();
    let kws = keyword_positions(&lower);
    if kws.is_empty() {
        return None;
    }
    let mut best: Option<(usize, String)> = None;
    let mut consider = |pos: usize, code: String, penalty: usize| {
        let dist = kws
            .iter()
            .map(|&k| if k <= pos { pos - k } else { (k - pos) + 20 })
            .min()
            .unwrap_or(usize::MAX);
        if dist > 250 {
            return;
        }
        let score = dist + penalty;
        if best.as_ref().map_or(true, |(s, _)| score < *s) {
            best = Some((score, code));
        }
    };

    for m in digits_re().find_iter(text) {
        let (s, e) = (m.start(), m.end());
        let prev = text[..s].chars().next_back();
        let next = text[e..].chars().next();
        let next2 = text[e..].chars().nth(1);
        // inside URLs, ids, decimals, money, percentages
        if matches!(prev, Some('/' | '=' | '#' | '&' | '@' | '$' | '€' | '£' | '₽' | '.' | ',')) { continue; }
        if matches!(next, Some('%' | '/' | '=' | '@')) { continue; }
        if matches!(next, Some('.' | ',')) && next2.map_or(false, |c| c.is_ascii_digit()) { continue; }
        let token_start = text[..s].rfind(char::is_whitespace).map_or(0, |i| i + 1);
        let token_end = text[e..].find(char::is_whitespace).map_or(text.len(), |i| e + i);
        let token = &text[token_start..token_end];
        if token.contains("http") || token.contains("www.") || token.contains('/') { continue; }
        let around = &lower[s.saturating_sub(6)..s];
        if around.contains("usd") || around.contains("uzs") || around.contains("sum") || around.contains("so'm") { continue; }

        let digits: String = m.as_str().chars().filter(|c| c.is_ascii_digit()).collect();
        let mut penalty = if digits.len() == 6 { 0 } else { 15 };
        if digits.len() == 4 {
            if let Ok(n) = digits.parse::<u32>() {
                if (1900..=2100).contains(&n) { penalty += 80; }
            }
        }
        consider(s, digits, penalty);
    }

    // Alphanumeric codes like K7Q2XP: 6-8 uppercase letters/digits with at least two of each.
    let mut offset = 0;
    for word in text.split(|c: char| !c.is_alphanumeric()) {
        let pos = text[offset..].find(word).map_or(offset, |i| offset + i);
        offset = pos + word.len();
        let n = word.chars().count();
        if !(6..=8).contains(&n) || !word.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) { continue; }
        let d = word.chars().filter(|c| c.is_ascii_digit()).count();
        if d < 2 || n - d < 2 { continue; }
        consider(pos, word.to_string(), 40);
    }
    best
}

/// The one-time code in an email, if it looks like one. Subject wins over body.
pub fn find_code(subject: &str, body: &str) -> Option<String> {
    if let Some((_, c)) = best_in(subject) {
        return Some(c);
    }
    best_in(body).map(|(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::find_code;

    #[test]
    fn common_shapes() {
        assert_eq!(find_code("G-482913 is your Google verification code", ""), Some("482913".into()));
        assert_eq!(find_code("Your code", "Your verification code is 123 456. It expires in 10 minutes."), Some("123456".into()));
        assert_eq!(find_code("Sign in to Steam", "Here is the Steam Guard code you need: K7Q2XP"), Some("K7Q2XP".into()));
        assert_eq!(find_code("Ваш код", "Код подтверждения: 5521"), Some("5521".into()));
        assert_eq!(find_code("Login attempt", "Use 849201 to log in. https://example.com/a/123456"), Some("849201".into()));
    }

    #[test]
    fn not_codes() {
        assert_eq!(find_code("Your order", "Total 1499.00 USD, order 20260911"), None);
        assert_eq!(find_code("Weekly digest", "In 2026 we shipped 1234 features"), None);
        assert_eq!(find_code("Receipt", "Paid $4999 for your subscription code red"), None);
    }
}
