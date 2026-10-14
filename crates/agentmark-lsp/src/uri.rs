//! `file://` URIs <-> the display paths the compiler uses.

use std::str::FromStr;

use lsp_types::Uri;

/// The path a `file://` URI names (percent-decoded). `None` for any other scheme.
pub fn path_from_uri(uri: &Uri) -> Option<String> {
    let rest = uri.as_str().strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The `file://` URI of an absolute path.
pub fn uri_from_path(path: &str) -> Uri {
    let mut s = String::from("file://");
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => s.push(b as char),
            _ => s.push_str(&format!("%{b:02X}")),
        }
    }
    Uri::from_str(&s).unwrap_or_else(|_| Uri::from_str("file:///").expect("valid uri"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_spaces_and_umlauts() {
        let p = "/tmp/my project/größe.agent.md";
        let uri = uri_from_path(p);
        assert_eq!(path_from_uri(&uri).as_deref(), Some(p));
    }
}
