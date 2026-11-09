//! Schriftverwaltung: Systemschriften zur **Laufzeit** indizieren und laden, Auflösung nach
//! PostScript-/Familien-/Stilnamen wie das Original (09 §6.6), Rückfall-Schrift.
//!
//! Es werden keine Schriften eingebettet. [`FontDb::system`] durchsucht `%WINDIR%\Fonts`, den
//! Benutzer-Schriftordner (`%LOCALAPPDATA%\Microsoft\Windows\Fonts`) und die Ordner aus der
//! Umgebungsvariablen `FP_FONT_DIRS` (durch `;` getrennt); unter macOS die System-, Rechner- und
//! Benutzerordner, unter Linux `/usr/share/fonts`, `/usr/local/share/fonts`, `~/.local/share/fonts`
//! und `~/.fonts` samt Unterordnern. Der Index liest nur Tabellenverzeichnis,
//! `name`, `OS/2` und `head`; die Schrift selbst wird erst bei Bedarf geladen und geteilt.
//!
//! Auflösung ([`FontDb::resolve`]):
//! 1. PostScript-Name (`TFSv/FONs`, `CFTs`), ohne Groß-/Kleinschreibung und Leerzeichen (B: das Original
//!    speichert und sucht den PostScript-Namen);
//! 2. Familienname (+ Stil fett/kursiv), z. B. der Anzeigename aus `TXTv/FONs`;
//! 3. PostScript-Name der Form `Familie-Stil` gegen Familie (ohne Leerzeichen) und Stil;
//! 4. Rückfall-Schrift (Vorgabe Myriad Pro, wie die Ersatzschrift von ATE, 09 §5.3), dann Arial,
//!    Segoe UI, Tahoma, Liberation Sans, DejaVu Sans, sonst die erste indizierte Schrift.
//!
//! Type-1-Schriften (`.pfb`/`.pfm`) und Bitmap-Schriften (`.fon`) werden nicht unterstützt.

pub mod face;
pub mod outline;
pub(crate) mod sfnt;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub use face::Font;
pub use outline::{GlyphOutline, Segment};
pub use sfnt::FontNames;

/// Eintrag des Schriftindex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaceInfo {
    /// Datei (oder leer bei Schriften aus dem Speicher).
    pub path: PathBuf,
    /// Index in einer Sammlung.
    pub index: u32,
    /// Namen.
    pub names: FontNames,
    /// `OS/2.usWeightClass`.
    pub weight: u16,
    /// Fett laut Stilangaben.
    pub bold: bool,
    /// Kursiv laut Stilangaben.
    pub italic: bool,
}

/// Ergebnis einer Schriftauflösung.
#[derive(Debug, Clone)]
pub struct ResolvedFont {
    /// Die geladene Schrift.
    pub font: Arc<Font>,
    /// `true`, wenn der gesuchte Name genau gefunden wurde (PostScript- oder Familie+Stil).
    pub exact: bool,
    /// Ersatzschrift (der gesuchte Name war nicht vorhanden).
    pub substituted: bool,
}

/// Normalisierter Schlüssel: Kleinbuchstaben ohne Leerzeichen, Bindestriche und Unterstriche.
fn key(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, ' ' | '-' | '_'))
        .flat_map(char::to_lowercase)
        .collect()
}

/// Normalisierter PostScript-Name (Kleinbuchstaben ohne Leerzeichen; Bindestrich bleibt).
fn ps_key(s: &str) -> String {
    s.chars()
        .filter(|c| *c != ' ')
        .flat_map(char::to_lowercase)
        .collect()
}

/// Stilwörter als (fett, kursiv) bzw. `None` bei unbekanntem Stil.
fn style_flags(style: &str) -> (bool, bool) {
    let s = style.to_lowercase();
    let bold = s.contains("bold") || s.contains("black") || s.contains("heavy") || s.contains("fett");
    let italic = s.contains("italic") || s.contains("oblique") || s.ends_with("it") || s.contains("kursiv");
    (bold, italic)
}

/// Index der verfügbaren Schriften mit Ladecache. Thread-sicher (`Send + Sync`).
#[derive(Default)]
pub struct FontDb {
    faces: Vec<FaceInfo>,
    memory: HashMap<usize, Arc<Font>>,
    by_ps: HashMap<String, usize>,
    by_family: HashMap<String, Vec<usize>>,
    loaded: Mutex<HashMap<usize, Option<Arc<Font>>>>,
    fallback_names: Vec<String>,
}

impl std::fmt::Debug for FontDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontDb")
            .field("faces", &self.faces.len())
            .finish_non_exhaustive()
    }
}

/// Ein Ordner und seine Unterordner (sortiert, höchstens vier Ebenen tief) für [`FontDb::system_dirs`].
fn collect_font_dirs(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    out.push(dir.to_path_buf());
    if depth >= 4 {
        return;
    }
    let mut subs: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    subs.sort();
    for d in subs {
        collect_font_dirs(&d, depth + 1, out);
    }
}

impl FontDb {
    /// Leerer Index mit den Standard-Rückfallnamen.
    pub fn new() -> Self {
        Self {
            fallback_names: [
                "MyriadPro-Regular",
                "ArialMT",
                "SegoeUI",
                "Tahoma",
                "LiberationSans",
                "DejaVuSans",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            ..Self::default()
        }
    }

    /// Standardordner der Systemschriften (siehe Moduldoku).
    pub fn system_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if cfg!(target_os = "macos") {
            // macOS: System-, Zusatz-, Rechner- und Benutzerschriften (nicht rekursiv gelesen)
            dirs.extend(
                [
                    "/System/Library/Fonts",
                    "/System/Library/Fonts/Supplemental",
                    "/Library/Fonts",
                ]
                .map(PathBuf::from),
            );
        } else if cfg!(unix) {
            // Linux/BSD: System-, lokale und Benutzerschriften; die Ordner sind dort verschachtelt
            // (z. B. truetype/dejavu), daher mit allen Unterordnern
            let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
            let home = env("HOME");
            let mut roots = vec![
                PathBuf::from("/usr/share/fonts"),
                PathBuf::from("/usr/local/share/fonts"),
            ];
            roots.extend(
                env("XDG_DATA_HOME")
                    .or_else(|| home.as_ref().map(|h| h.join(".local").join("share")))
                    .map(|d| d.join("fonts")),
            );
            roots.extend(home.map(|h| h.join(".fonts")));
            for r in roots {
                collect_font_dirs(&r, 0, &mut dirs);
            }
        } else {
            let windir = std::env::var_os("WINDIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
            dirs.push(windir.join("Fonts"));
            if let Some(l) = std::env::var_os("LOCALAPPDATA") {
                dirs.push(PathBuf::from(l).join("Microsoft").join("Windows").join("Fonts"));
            }
        }
        if let Some(extra) = std::env::var_os("FP_FONT_DIRS") {
            dirs.extend(std::env::split_paths(&extra));
        }
        dirs
    }

    /// Index über alle Systemschriften (siehe [`FontDb::system_dirs`]).
    pub fn system() -> Self {
        let mut db = Self::new();
        for d in Self::system_dirs() {
            db.add_dir(&d);
        }
        db
    }

    fn slot() -> &'static std::sync::RwLock<Arc<FontDb>> {
        static DB: std::sync::OnceLock<std::sync::RwLock<Arc<FontDb>>> = std::sync::OnceLock::new();
        DB.get_or_init(|| std::sync::RwLock::new(Arc::new(FontDb::system())))
    }

    /// The fonts everything uses (the system's, indexed on first use).
    pub fn shared() -> Arc<FontDb> {
        match Self::slot().read() {
            Ok(db) => db.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Replace the fonts everything uses (a chosen folder added, or the browser's).
    pub fn set_shared(db: FontDb) {
        match Self::slot().write() {
            Ok(mut slot) => *slot = Arc::new(db),
            Err(poisoned) => *poisoned.into_inner() = Arc::new(db),
        }
    }

    /// Rückfallnamen (PostScript- oder Familiennamen) in Reihenfolge setzen.
    pub fn set_fallbacks(&mut self, names: &[&str]) {
        self.fallback_names = names.iter().map(|s| s.to_string()).collect();
    }

    /// Alle Schriftdateien eines Ordners (nicht rekursiv, sortiert) aufnehmen.
    pub fn add_dir(&mut self, dir: &Path) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut files: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
        files.sort_by_key(|p| p.file_name().map(|n| n.to_ascii_lowercase()));
        for p in files {
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            if matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc") {
                self.add_file(&p);
            }
        }
    }

    /// Eine Schriftdatei (auch Sammlung) aufnehmen; liefert die Anzahl neuer Einträge.
    pub fn add_file(&mut self, path: &Path) -> usize {
        let qs = sfnt::quick_scan(path);
        let n = qs.len();
        for q in qs {
            self.push(FaceInfo {
                path: path.to_path_buf(),
                index: q.index,
                names: q.names,
                weight: q.weight,
                bold: q.bold,
                italic: q.italic,
            });
        }
        n
    }

    /// Schrift aus dem Speicher aufnehmen (z. B. für Tests oder eingebettete Dokumentschriften).
    pub fn add_font(&mut self, font: Font) -> usize {
        let (bold, italic) = style_flags(font.subfamily());
        let info = FaceInfo {
            path: PathBuf::new(),
            index: font.index(),
            names: FontNames {
                family: font.family().to_string(),
                subfamily: font.subfamily().to_string(),
                ps_name: font.ps_name().to_string(),
                ..FontNames::default()
            },
            weight: if bold { 700 } else { 400 },
            bold,
            italic,
        };
        let id = self.push(info);
        self.memory.insert(id, Arc::new(font));
        id
    }

    fn push(&mut self, info: FaceInfo) -> usize {
        let id = self.faces.len();
        if !info.names.ps_name.is_empty() {
            self.by_ps.entry(ps_key(&info.names.ps_name)).or_insert(id);
        }
        for fam in [&info.names.family, &info.names.legacy_family] {
            if !fam.is_empty() {
                let v = self.by_family.entry(key(fam)).or_default();
                if !v.contains(&id) {
                    v.push(id);
                }
            }
        }
        self.faces.push(info);
        id
    }

    /// Alle Indexeinträge.
    pub fn faces(&self) -> &[FaceInfo] {
        &self.faces
    }

    /// Anzahl der Einträge.
    pub fn len(&self) -> usize {
        self.faces.len()
    }

    /// Leer?
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// Namen aller Familien (sortiert, ohne Doppelte) für die Schriftauswahl der UI.
    pub fn families(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .faces
            .iter()
            .map(|f| f.names.family.clone())
            .filter(|s| !s.is_empty())
            .collect();
        v.sort_by_key(|s| s.to_lowercase());
        v.dedup();
        v
    }

    /// Lädt Eintrag `id` (zwischengespeichert; defekte Dateien → `None`).
    pub fn load(&self, id: usize) -> Option<Arc<Font>> {
        if let Some(f) = self.memory.get(&id) {
            return Some(f.clone());
        }
        let mut cache = self.loaded.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(f) = cache.get(&id) {
            return f.clone();
        }
        let info = self.faces.get(id)?;
        let f = Font::from_file(&info.path, info.index).ok().map(Arc::new);
        cache.insert(id, f.clone());
        f
    }

    /// Sucht den Eintrag zu einem PostScript-Namen.
    pub fn find_ps(&self, name: &str) -> Option<usize> {
        self.by_ps.get(&ps_key(name)).copied()
    }

    /// Sucht in einer Familie den Stil mit (fett, kursiv); `exact = false`, wenn nur ein anderer Stil
    /// der Familie existiert (dann der regulärste).
    pub fn find_family(&self, family: &str, bold: bool, italic: bool) -> Option<(usize, bool)> {
        let ids = self.by_family.get(&key(family))?;
        let score = |i: usize| {
            let f = &self.faces[i];
            let fb = f.bold || f.weight >= 600;
            let sub = f.names.subfamily.to_lowercase();
            let width_penalty = ["cond", "narrow", "semi", "light", "black", "medium"]
                .iter()
                .any(|w| sub.contains(w));
            let w = (fb != bold) as i32 * 4 + (f.italic != italic) as i32 * 4 + width_penalty as i32;
            (w, (f.weight as i32 - if bold { 700 } else { 400 }).abs())
        };
        let best = ids.iter().copied().min_by_key(|&i| score(i))?;
        let (w, _) = score(best);
        Some((best, w == 0))
    }

    /// Löst einen Schriftnamen auf (Reihenfolge siehe Moduldoku). `bold`/`italic` wählen nur bei der
    /// Familiensuche den Stil; Faux-Fett/-Kursiv aus den Laufattributen bleiben davon unberührt.
    pub fn resolve(&self, name: &str, bold: bool, italic: bool) -> Option<ResolvedFont> {
        if let Some(id) = self.find_ps(name)
            && let Some(font) = self.load(id)
        {
            return Some(ResolvedFont {
                font,
                exact: true,
                substituted: false,
            });
        }
        if let Some((id, exact)) = self.find_family(name, bold, italic)
            && let Some(font) = self.load(id)
        {
            return Some(ResolvedFont {
                font,
                exact,
                substituted: false,
            });
        }
        if let Some((fam, style)) = name.split_once('-') {
            let (b, i) = style_flags(style);
            if let Some((id, exact)) = self.find_family(fam, b || bold, i || italic)
                && let Some(font) = self.load(id)
            {
                return Some(ResolvedFont {
                    font,
                    exact,
                    substituted: false,
                });
            }
        }
        self.fallback().map(|font| ResolvedFont {
            font,
            exact: false,
            substituted: true,
        })
    }

    /// Die Rückfall-Schrift (erste vorhandene der Rückfallnamen, sonst die erste ladbare Schrift).
    pub fn fallback(&self) -> Option<Arc<Font>> {
        for n in &self.fallback_names {
            if let Some(id) = self
                .find_ps(n)
                .or_else(|| self.find_family(n, false, false).map(|x| x.0))
                && let Some(f) = self.load(id)
            {
                return Some(f);
            }
        }
        (0..self.faces.len()).find_map(|i| self.load(i))
    }
}
