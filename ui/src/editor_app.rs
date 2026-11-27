//! The text editor on `App`'s side: `EditorKit`'s callbacks over `editor.rs`.

use crate::editor;
use crate::{App, AppWindow, EditorEdit, EditorKit, EditorMatch};
use slint::{ComponentHandle, SharedString};
use std::sync::Arc;

fn offset(v: i32) -> usize {
    v.max(0) as usize
}

fn found(query: &str, hit: Option<editor::Hit>) -> EditorMatch {
    let label: SharedString = editor::hit_label(query, hit).into();
    match hit {
        Some(h) => EditorMatch {
            start: h.start as i32,
            end: h.end as i32,
            count: h.count as i32,
            label,
        },
        None => EditorMatch {
            start: -1,
            end: -1,
            count: 0,
            label,
        },
    }
}

fn edit(text: String, caret: usize) -> EditorEdit {
    EditorEdit {
        text: text.into(),
        caret: caret as i32,
        ok: true,
    }
}

impl App {
    pub(crate) fn bind_editor(self: &Arc<Self>, ui: &AppWindow) {
        let kit = ui.global::<EditorKit>();
        kit.on_find(|text, query, match_case, from, back| {
            found(
                &query,
                editor::find(&text, &query, match_case, offset(from), back),
            )
        });
        kit.on_replace(|text, query, match_case, at, with| {
            match editor::replace_at(&text, &query, match_case, offset(at), &with) {
                Some((text, caret)) => edit(text, caret),
                None => EditorEdit::default(),
            }
        });
        kit.on_replace_all(|text, query, match_case, with| {
            match editor::replace_all(&text, &query, match_case, &with) {
                (_, 0) => EditorEdit::default(),
                (text, _) => edit(text, 0),
            }
        });
    }
}
