//! The text editor on `App`'s side: `EditorKit`'s callbacks over `editor.rs`, and each
//! open editor's history by the page's key.

use crate::editor;
use crate::editor_history::History;
use crate::{App, AppWindow, EditorEdit, EditorKit, EditorMatch, EditorState};
use slint::{ComponentHandle, SharedString};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// How long typing stops before what came before it is one undo step.
const PAUSE: Duration = Duration::from_millis(800);

fn histories() -> &'static Mutex<HashMap<String, (History, Instant)>> {
    static H: OnceLock<Mutex<HashMap<String, (History, Instant)>>> = OnceLock::new();
    H.get_or_init(Default::default)
}

fn with_history<T>(key: &str, f: impl FnOnce(&mut History) -> T) -> T {
    let mut all = histories().lock().unwrap();
    let (history, _) = all
        .entry(key.to_string())
        .or_insert_with(|| (History::default(), Instant::now()));
    f(history)
}

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

fn replaced(key: &str, before: &str, after: String, caret: usize) -> EditorEdit {
    with_history(key, |h| h.replaced(before, &after));
    edit(after, caret)
}

impl App {
    pub(crate) fn bind_editor(self: &Arc<Self>, ui: &AppWindow) {
        let kit = ui.global::<EditorKit>();
        kit.on_opened(|key, text| {
            histories()
                .lock()
                .unwrap()
                .insert(key.to_string(), (History::new(&text), Instant::now()));
        });
        kit.on_typed(|key, text| {
            let mut all = histories().lock().unwrap();
            let (history, last) = all
                .entry(key.to_string())
                .or_insert_with(|| (History::default(), Instant::now()));
            history.typed(&text, last.elapsed() >= PAUSE);
            *last = Instant::now();
        });
        kit.on_history(|key, text| {
            with_history(&key, |h| EditorState {
                undo: h.can_undo(&text),
                redo: h.can_redo(&text),
            })
        });
        kit.on_undo(|key, text| match with_history(&key, |h| h.undo(&text)) {
            Some((text, caret)) => edit(text, caret),
            None => EditorEdit::default(),
        });
        kit.on_redo(|key, text| match with_history(&key, |h| h.redo(&text)) {
            Some((text, caret)) => edit(text, caret),
            None => EditorEdit::default(),
        });
        kit.on_find(|text, query, match_case, from, back| {
            found(
                &query,
                editor::find(&text, &query, match_case, offset(from), back),
            )
        });
        kit.on_replace(|key, text, query, match_case, at, with| {
            match editor::replace_at(&text, &query, match_case, offset(at), &with) {
                Some((after, caret)) => replaced(&key, &text, after, caret),
                None => EditorEdit::default(),
            }
        });
        kit.on_replace_all(|key, text, query, match_case, with| {
            match editor::replace_all(&text, &query, match_case, &with) {
                (_, 0) => EditorEdit::default(),
                (after, _) => replaced(&key, &text, after, 0),
            }
        });
    }
}
