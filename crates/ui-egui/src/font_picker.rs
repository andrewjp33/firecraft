//! The font drop-down: while its list is open, typing finds a font by name, as in Fireworks.

use std::hash::Hash;

use egui::{Align, ComboBox, Event, EventFilter, Id, Key, Rect, Sense, Ui, Vec2};

/// Letters typed further apart than this (seconds) start a new search.
const TYPING_GAP: f64 = 1.0;
/// How many rows Page Up and Page Down move.
const PAGE: usize = 12;

/// The open list's state.
#[derive(Clone, Default)]
struct Find {
    typed: String,
    typed_at: f64,
    row: usize,
    scroll: bool,
}

/// The row to go to for what's been `typed`, searching round the list from `row`.
fn find(fonts: &[String], typed: &str, row: usize) -> Option<usize> {
    let typed = typed.to_lowercase();
    let first = typed.chars().next()?;
    let search = |from: usize, prefix: &str| {
        (0..fonts.len())
            .map(|k| (from + k) % fonts.len())
            .find(|&i| fonts.get(i).is_some_and(|f| f.to_lowercase().starts_with(prefix)))
    };
    // One letter moves on from the current row, so pressing it again steps through its fonts.
    if typed.chars().count() == 1 {
        return search(row + 1, &typed);
    }
    search(row, &typed).or_else(|| {
        typed
            .chars()
            .all(|c| c == first)
            .then(|| search(row + 1, &first.to_string()))
            .flatten()
    })
}

/// A drop-down of `fonts`: the font picked, if any.
pub fn font_combo(
    ui: &mut Ui,
    salt: impl Hash + Copy + std::fmt::Debug,
    width: f32,
    height: f32,
    current: &str,
    fonts: &[String],
) -> Option<String> {
    let state = Id::unique(("font_find", salt));
    let mut picked = None;
    let shown = ComboBox::from_id_salt(salt)
        .width(width)
        .height(height)
        .selected_text(current)
        .show_ui(ui, |ui| {
            let mut f: Find = ui.data(|d| d.get_temp(state)).unwrap_or_else(|| Find {
                row: fonts.iter().position(|n| n == current).unwrap_or(0),
                scroll: true,
                ..Find::default()
            });
            // Holding the keyboard keeps typing away from tool shortcuts and the text being edited.
            let keys = ui.interact(
                Rect::from_min_size(ui.min_rect().min, Vec2::ZERO),
                state.with("keys"),
                Sense::focusable_noninteractive(),
            );
            keys.request_focus();
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    keys.id,
                    EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: false,
                    },
                );
            });
            let last = fonts.len().saturating_sub(1);
            let (now, events) = ui.input(|i| (i.time, i.events.clone()));
            let mut enter = false;
            for event in events {
                let moved = match event {
                    Event::Text(s) if !s.chars().any(char::is_control) => {
                        if now - f.typed_at > TYPING_GAP {
                            f.typed.clear();
                        }
                        if f.typed.is_empty() && s.trim().is_empty() {
                            continue;
                        }
                        f.typed.push_str(&s);
                        f.typed_at = now;
                        match find(fonts, &f.typed, f.row) {
                            Some(row) => f.row = row,
                            None => continue,
                        }
                        f.scroll = true;
                        continue;
                    }
                    Event::Key {
                        key, pressed: true, ..
                    } => match key {
                        Key::ArrowDown => Some((f.row + 1).min(last)),
                        Key::ArrowUp => Some(f.row.saturating_sub(1)),
                        Key::PageDown => Some((f.row + PAGE).min(last)),
                        Key::PageUp => Some(f.row.saturating_sub(PAGE)),
                        Key::Home => Some(0),
                        Key::End => Some(last),
                        Key::Enter => {
                            enter = true;
                            None
                        }
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(row) = moved {
                    f.row = row;
                    f.typed.clear();
                    f.scroll = true;
                }
            }
            for (i, name) in fonts.iter().enumerate() {
                let row = ui.selectable_label(i == f.row, name);
                if row.clicked() {
                    picked = Some(name.clone());
                }
                if i == f.row && f.scroll {
                    row.scroll_to_me(Some(Align::Center));
                }
            }
            if !ui.is_sizing_pass() {
                f.scroll = false;
            }
            if enter {
                picked = fonts.get(f.row).cloned();
                ui.close();
            }
            ui.data_mut(|d| d.insert_temp(state, f));
        });
    if shown.inner.is_none() {
        ui.data_mut(|d| d.remove::<Find>(state));
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::find;

    fn fonts() -> Vec<String> {
        [
            "Arial",
            "Courier New",
            "Tahoma",
            "Times New Roman",
            "Trebuchet MS",
        ]
        .map(String::from)
        .to_vec()
    }

    #[test]
    fn a_letter_goes_to_the_first_font_after_the_current_one() {
        assert_eq!(find(&fonts(), "t", 0), Some(2));
        assert_eq!(find(&fonts(), "T", 0), Some(2), "any case");
    }

    #[test]
    fn pressing_a_letter_again_steps_through_its_fonts_and_wraps() {
        assert_eq!(find(&fonts(), "t", 2), Some(3));
        assert_eq!(find(&fonts(), "tt", 3), Some(4));
        assert_eq!(find(&fonts(), "t", 4), Some(2));
    }

    #[test]
    fn several_letters_narrow_it_down_and_stay_put_while_they_match() {
        assert_eq!(find(&fonts(), "tr", 2), Some(4));
        assert_eq!(find(&fonts(), "tre", 4), Some(4));
        assert_eq!(find(&fonts(), "times n", 3), Some(3));
    }

    #[test]
    fn no_match_leaves_the_row_alone() {
        assert_eq!(find(&fonts(), "z", 1), None);
        assert_eq!(find(&fonts(), "tz", 2), None);
        assert_eq!(find(&[], "a", 0), None);
    }
}
