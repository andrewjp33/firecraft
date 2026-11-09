//! Cursor- und Auswahl-Abfragen für den Text-Editor (fp-ui).
//!
//! Zeichenindizes zählen Unicode-Skalare im Gesamttext (Verkettung aller Läufe, Steuerzeichen
//! eingeschlossen). Ein Caret-Index `i` liegt **vor** Zeichen `i`; `char_count` ist das Textende.
//! Für waagerechten Text (Punkttext, Rahmen, Form) gelten die Zeilen-Carets exakt; bei Text auf
//! Pfad und vertikalem Text werden Glyphenmitten herangezogen (Näherung).

use crate::support::Rect;

use super::types::TextLayout;
use crate::model::FrameKind;

/// Lage eines Carets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Caret {
    /// Zeile.
    pub line: usize,
    /// Oberes Ende (Dokumentkoordinaten).
    pub top: [f64; 2],
    /// Unteres Ende.
    pub bottom: [f64; 2],
}

/// Ergebnis eines Treffertests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitResult {
    /// Caret-Index (vor diesem Zeichen).
    pub char_index: usize,
    /// Zeile.
    pub line: usize,
    /// Punkt lag innerhalb einer Glyphenbox.
    pub inside: bool,
}

impl TextLayout {
    fn is_linear(&self) -> bool {
        self.kind != FrameKind::OnPath && !self.orientation.is_vertical()
    }

    /// Caret-Index am Punkt (x, y): nächste Zeile nach y, darin die nächste Caret-Position nach x.
    pub fn hit_test(&self, x: f64, y: f64) -> HitResult {
        if self.lines.is_empty() {
            return HitResult {
                char_index: 0,
                line: 0,
                inside: false,
            };
        }
        if !self.is_linear() {
            return self.hit_test_glyphs(x, y);
        }
        // Zeile: y innerhalb [Grundlinie − Oberlänge, Grundlinie + Unterlänge], sonst die nächste
        let mut best = (f64::INFINITY, 0usize);
        for (i, l) in self.lines.iter().enumerate() {
            let (t, b) = (l.baseline - l.ascent, l.baseline + l.descent);
            let d = if y < t {
                t - y
            } else if y > b {
                y - b
            } else {
                0.0
            };
            if d < best.0 {
                best = (d, i);
            }
        }
        let li = best.1;
        let l = &self.lines[li];
        // letzte Caret-Position einer Zeile mit Umbruchzeichen liegt vor dem Umbruch
        let n = l.carets.len();
        let has_break = li + 1 < self.lines.len();
        let max_k = if has_break && n >= 2 { n - 2 } else { n - 1 };
        let mut bk = 0usize;
        let mut bd = f64::INFINITY;
        for (k, &cx) in l.carets.iter().enumerate().take(max_k + 1) {
            let d = (cx - x).abs();
            if d < bd {
                bd = d;
                bk = k;
            }
        }
        let inside = best.0 == 0.0 && x >= l.start && x <= l.start + l.width;
        HitResult {
            char_index: (l.chars.start + bk).min(self.char_count),
            line: li,
            inside,
        }
    }

    fn hit_test_glyphs(&self, x: f64, y: f64) -> HitResult {
        let mut best: Option<(f64, usize, usize, bool)> = None;
        for g in self.glyphs.iter().filter(|g| g.visible) {
            let Some(r) = g.ink_bounds().or_else(|| {
                let p = g.transform.apply([0.0, 0.0]);
                Some(Rect::new(p[0] as f32, p[1] as f32, p[0] as f32, p[1] as f32))
            }) else {
                continue;
            };
            let (cx, cy) = (
                ((r.left + r.right) / 2.0) as f64,
                ((r.top + r.bottom) / 2.0) as f64,
            );
            let d = (cx - x).hypot(cy - y);
            // vordere oder hintere Hälfte der Glyphe entlang ihrer Laufrichtung
            let o = g.transform.apply([0.0, 0.0]);
            let dir = [g.transform.a, g.transform.b];
            let along = (x - o[0]) * dir[0] + (y - o[1]) * dir[1];
            let adv_units = (g.advance - g.tracking) / (g.size / g.font.units_per_em() as f64);
            let after = along > adv_units * (dir[0] * dir[0] + dir[1] * dir[1]) / 2.0;
            let inside =
                x >= r.left as f64 && x <= r.right as f64 && y >= r.top as f64 && y <= r.bottom as f64;
            if best.is_none_or(|b| d < b.0) {
                let ci = if after {
                    g.char_index + g.char_len.max(1)
                } else {
                    g.char_index
                };
                best = Some((d, ci, g.line, inside));
            }
        }
        match best {
            Some((_, ci, line, inside)) => HitResult {
                char_index: ci.min(self.char_count),
                line,
                inside,
            },
            None => HitResult {
                char_index: 0,
                line: 0,
                inside: false,
            },
        }
    }

    /// Caret-Lage für den Index (vor Zeichen `char_index`; `char_count` = Textende).
    pub fn caret(&self, char_index: usize) -> Option<Caret> {
        let li = self.line_of_char(char_index)?;
        let l = &self.lines[li];
        if !self.is_linear() {
            let g = self
                .glyphs
                .iter()
                .find(|g| g.char_index >= char_index)
                .or_else(|| self.glyphs.last())?;
            let k = g.size / g.font.units_per_em() as f64;
            let upm = g.font.units_per_em() as f64;
            let x_units = if g.char_index >= char_index {
                0.0
            } else {
                (g.advance - g.tracking) / k
            };
            let top = g.transform.apply([x_units, g.font.ascent() * upm]);
            let bottom = g.transform.apply([x_units, -g.font.descent() * upm]);
            return Some(Caret {
                line: g.line,
                top,
                bottom,
            });
        }
        let k = char_index
            .saturating_sub(l.chars.start)
            .min(l.carets.len().saturating_sub(1));
        let x = l.carets.get(k).copied().unwrap_or(l.start);
        Some(Caret {
            line: li,
            top: [x, l.baseline - l.ascent],
            bottom: [x, l.baseline + l.descent],
        })
    }

    /// Rechtecke der Auswahl `[start, end)` (eins je betroffener Zeile, waagerechter Text) bzw. die
    /// Glyphenboxen (Pfad-/Vertikaltext).
    pub fn selection_rects(&self, start: usize, end: usize) -> Vec<Rect> {
        let (s, e) = if start <= end { (start, end) } else { (end, start) };
        let mut out = Vec::new();
        if s == e {
            return out;
        }
        if !self.is_linear() {
            for g in self
                .glyphs
                .iter()
                .filter(|g| g.visible && g.char_index >= s && g.char_index < e)
            {
                if let Some(r) = g.ink_bounds() {
                    out.push(r);
                }
            }
            return out;
        }
        for l in &self.lines {
            let (a, b) = (s.max(l.chars.start), e.min(l.chars.end));
            if a >= b {
                continue;
            }
            let ka = a - l.chars.start;
            let kb = (b - l.chars.start).min(l.carets.len() - 1);
            let (x0, x1) = (l.carets[ka], l.carets[kb]);
            let (x0, x1) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
            // Umbruchzeichen in der Auswahl: bis zum Zeilenende plus ein kleiner Rand
            let x1 = if b == l.chars.end && b > a && x1 <= x0 {
                x0 + 4.0
            } else {
                x1
            };
            out.push(Rect::new(
                x0 as f32,
                (l.baseline - l.ascent) as f32,
                x1 as f32,
                (l.baseline + l.descent) as f32,
            ));
        }
        out
    }

    /// Zeichenbereich der Zeile, die `char_index` enthält (für Pos1/Ende im Editor).
    pub fn line_range(&self, char_index: usize) -> Option<std::ops::Range<usize>> {
        self.line_of_char(char_index).map(|i| self.lines[i].chars.clone())
    }

    /// Caret-Index eine Zeile höher (`delta = −1`) bzw. tiefer (`+1`) an derselben x-Position.
    pub fn move_vertical(&self, char_index: usize, delta: i32) -> usize {
        let Some(li) = self.line_of_char(char_index) else {
            return char_index;
        };
        let target = li as i64 + delta as i64;
        if target < 0 {
            return 0;
        }
        if target as usize >= self.lines.len() {
            return self.char_count;
        }
        let Some(c) = self.caret(char_index) else {
            return char_index;
        };
        let l = &self.lines[target as usize];
        self.hit_test(c.top[0], l.baseline).char_index
    }
}
