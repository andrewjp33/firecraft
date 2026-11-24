//! File > Open Recent: the files last opened or saved, newest first.
//!
//! Fireworks MX keeps MFC's recent-file list (`LoadStdProfileSettings(10)`: 10 files, menu
//! names shortened to 30 characters by `AbbreviateName`). A file comes to the top when it is
//! opened or saved; one that can't be opened any more is dropped from the list when chosen.
//! FireCraft adds Clear Recent Files.

/// Fireworks MX's list length (its `LoadStdProfileSettings` argument, read from Fireworks.exe).
pub const MAX: usize = 10;

/// How long a name in the menu may be before folders are left out (MFC's `nMaxDispLen`, which
/// Fireworks MX sets to 30).
pub const MAX_SHOWN: usize = 30;

/// Storage key of the list (one path per line).
pub const STORAGE_KEY: &str = "firecraft.recent";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecentFiles {
    paths: Vec<String>,
}

impl RecentFiles {
    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    /// Bring `path` to the top (opened or saved).
    pub fn add(&mut self, path: &str) {
        self.paths.retain(|p| !same_path(p, path));
        self.paths.insert(0, path.to_owned());
        self.paths.truncate(MAX);
    }

    /// Drop `path` (it could not be opened).
    pub fn remove(&mut self, path: &str) {
        self.paths.retain(|p| !same_path(p, path));
    }

    pub fn clear(&mut self) {
        self.paths.clear();
    }

    pub fn serialize(&self) -> String {
        self.paths.join("\n")
    }

    pub fn parse(text: &str) -> Self {
        let mut list = Self::default();
        for line in text.lines().rev() {
            let line = line.trim_end_matches('\r');
            if !line.is_empty() {
                list.add(line);
            }
        }
        list
    }
}

/// Whether two paths name the same file: Windows paths ignore case (as MFC's `AfxComparePath`).
fn same_path(a: &str, b: &str) -> bool {
    if a.contains('\\') || b.contains('\\') {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

/// A path shortened as MFC's `AbbreviateName` shortens it for the recent-file menu: whole while
/// it fits `max` characters; otherwise its volume (a drive with its first folder, `C:\Art`, or a
/// UNC `\\server\share`), `\...`, and as many of the last folders as fit before the file name
/// (`C:\Art\...\Buttons\nav.png`); just the file name when that doesn't fit either. A `/` path
/// is treated as MFC treats a drive's (`/home/.../probes/ball.png`).
pub fn abbreviate(path: &str, max: usize) -> String {
    let c: Vec<char> = path.chars().collect();
    let is_sep = |ch: char| ch == '/' || ch == '\\';
    let full = c.len();
    let name_at = c.iter().rposition(|&ch| is_sep(ch)).map_or(0, |i| i + 1);
    let name: String = c[name_at..].iter().collect();
    let name_len = full - name_at;
    if max >= full {
        return path.to_owned();
    }
    if max < name_len || name_at == 0 {
        return name;
    }
    let next_sep = |from: usize| (from..full).find(|&i| is_sep(c[i]));
    // The separator the volume ends at: after "C:", after "\\server", or the first one.
    let start = if full >= 2 && is_sep(c[0]) && is_sep(c[1]) {
        next_sep(2)
    } else if full >= 3 && c[1] == ':' && is_sep(c[2]) {
        Some(2)
    } else {
        next_sep(0)
    };
    let Some(mut cur) = start else { return name };
    // A UNC name's share, or a drive's first folder, belongs to the volume.
    if name_at - 1 > cur
        && let Some(i) = next_sep(cur + 1)
    {
        cur = i;
    }
    let vol = cur;
    if max < vol + 5 + name_len {
        return name;
    }
    // Leave out folders after the volume until "<volume>\...<rest>" fits.
    while vol + 4 + (full - cur) > max {
        match next_sep(cur + 1) {
            Some(i) => cur = i,
            None => break,
        }
    }
    let sep = c[cur];
    let volume: String = c[..vol].iter().collect();
    let rest: String = c[cur..].iter().collect();
    format!("{volume}{sep}...{rest}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_without_repeats_and_ten_at_most() {
        let mut r = RecentFiles::default();
        for i in 0..12 {
            r.add(&format!("/f/{i}.png"));
        }
        r.add("/f/5.png");
        assert_eq!(r.paths().len(), MAX);
        assert_eq!(r.paths()[0], "/f/5.png");
        assert_eq!(r.paths()[1], "/f/11.png");
        assert_eq!(r.paths().iter().filter(|p| *p == "/f/5.png").count(), 1);
        assert!(!r.paths().contains(&"/f/0.png".to_owned()));
        let back = RecentFiles::parse(&r.serialize());
        assert_eq!(back, r);
        r.remove("/f/5.png");
        assert_eq!(r.paths()[0], "/f/11.png");
    }

    #[test]
    fn windows_paths_ignore_case() {
        let mut r = RecentFiles::default();
        r.add(r"C:\Art\Logo.png");
        r.add(r"c:\art\logo.PNG");
        assert_eq!(r.paths(), [r"c:\art\logo.PNG"]);
    }

    #[test]
    fn long_paths_are_shortened_as_mfc_does() {
        assert_eq!(abbreviate(r"C:\Art\logo.png", 30), r"C:\Art\logo.png");
        assert_eq!(
            abbreviate(r"C:\Art\Projects\2002\Website\Buttons\nav.png", 30),
            r"C:\Art\...\Buttons\nav.png"
        );
        // The drive's first folder alone is too long: just the name.
        assert_eq!(abbreviate(r"C:\Documents and Settings\me\My Documents\logo.png", 30), "logo.png");
        assert_eq!(abbreviate("/home/me/Documents/work/probes/ball.png", 30), "/home/.../work/probes/ball.png");
        assert_eq!(abbreviate("/home/me/a-very-long-file-name-indeed.png", 30), "a-very-long-file-name-indeed.png");
        assert_eq!(
            abbreviate(r"\\server\share\deep\deeper\deepest\x.png", 30),
            r"\\server\share\...\x.png"
        );
    }
}
