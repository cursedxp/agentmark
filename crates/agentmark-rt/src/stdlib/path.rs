//! `path.*` (SPEC 22.3): text in, text out; nothing touches the disk.

use std::path::Path;

/// `path.join(a, b)`: `a/b` (just `b` when `a` is empty or `b` is absolute).
pub fn join(a: &str, b: &str) -> String {
    Path::new(a).join(b).to_string_lossy().into_owned()
}

/// `path.name(p)`: the last part, with its extension (`a/b.jpg` is `b.jpg`; empty when there is none).
pub fn name(p: &str) -> String {
    Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// `path.stem(p)`: the name without its extension (`b.jpg` is `b`).
pub fn stem(p: &str) -> String {
    Path::new(p).file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// `path.extension(p)`: the extension without the dot (`jpg`; empty when there is none).
pub fn extension(p: &str) -> String {
    Path::new(p).extension().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// `path.parent(p)`: everything before the last part (empty for a bare name).
pub fn parent(p: &str) -> String {
    Path::new(p).parent().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts() {
        assert_eq!(join("photos", "x.jpg"), "photos/x.jpg");
        assert_eq!(join("photos/", "x.jpg"), "photos/x.jpg");
        assert_eq!(join("", "x.jpg"), "x.jpg");
        assert_eq!(name("a/b/c.tar.gz"), "c.tar.gz");
        assert_eq!(stem("a/b/c.tar.gz"), "c.tar");
        assert_eq!(extension("a/b.jpg"), "jpg");
        assert_eq!(extension("a/b"), "");
        assert_eq!(extension(".hidden"), "");
        assert_eq!(parent("a/b/c.txt"), "a/b");
        assert_eq!(parent("c.txt"), "");
        assert_eq!(name(""), "");
    }
}
