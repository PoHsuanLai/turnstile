//! One line per event on standard error, which the journal collects: `turnstile: event key=value ...`.
//! A value that is not a plain word is quoted. Nothing here is shown to a person.

use std::fmt::Display;

/// One `key=value` field. A function so that any `Display` value coerces at the call.
pub fn kv<'a>(key: &'a str, value: &'a dyn Display) -> (&'a str, &'a dyn Display) {
    (key, value)
}

/// Builds the line.
pub fn line(event: &str, fields: &[(&str, &dyn Display)]) -> String {
    let mut out = format!("turnstile: {event}");
    for (key, value) in fields {
        let text = value.to_string();
        let plain = !text.is_empty()
            && text
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_./@:+".contains(c));
        if plain {
            out.push_str(&format!(" {key}={text}"));
        } else {
            out.push_str(&format!(" {key}={text:?}"));
        }
    }
    out
}

/// Writes the line.
pub fn note(event: &str, fields: &[(&str, &dyn Display)]) {
    eprintln!("{}", line(event, fields));
}
