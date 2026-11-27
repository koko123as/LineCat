//! The text editor on `App`'s side: `EditorKit`'s callbacks over `editor.rs`, and each
//! open editor's history by the page's key.

use crate::editor;
use crate::editor_complete::{self as complete, Completion};
use crate::editor_history::History;
use crate::settings_app::Effect;
use crate::{
    App, AppWindow, EditorCompletion, EditorEdit, EditorKit, EditorMatch, EditorSuggestion,
};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
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

fn completion_at(schema: &str, text: &str, caret: i32) -> Option<(usize, Completion)> {
    let caret = usize::try_from(caret).ok()?;
    complete::complete(complete::source_of(schema), text, caret).map(|c| (caret, c))
}

fn shown(found: Option<(usize, Completion)>) -> EditorCompletion {
    let Some((caret, c)) = found else {
        return EditorCompletion {
            items: ModelRc::default(),
            caret: -1,
            cells: 0.0,
        };
    };
    let items: Vec<EditorSuggestion> = c
        .items
        .iter()
        .map(|s| EditorSuggestion {
            label: s.label.as_str().into(),
            detail: s.detail.as_str().into(),
        })
        .collect();
    EditorCompletion {
        items: ModelRc::new(VecModel::from(items)),
        caret: caret as i32,
        cells: complete::cells(&c.items),
    }
}

impl App {
    pub(crate) fn bind_editor(self: &Arc<Self>, ui: &AppWindow) {
        let kit = ui.global::<EditorKit>();
        {
            let store = self.store.lock().unwrap();
            let s = &store.state.settings;
            kit.set_wrap(s.editor_line_wrap);
            kit.set_size(editor::font_level(s.editor_font_size) as i32);
            kit.set_font_px(editor::FONT_SIZES[editor::font_level(s.editor_font_size)].0);
        }
        let w = ui.as_weak();
        kit.on_menu(move |key, text, sizes_open| {
            let Some(ui) = w.upgrade() else {
                return ModelRc::default();
            };
            let kit = ui.global::<EditorKit>();
            let (undo, redo) = with_history(&key, |h| (h.can_undo(&text), h.can_redo(&text)));
            let items = editor::menu(undo, redo, kit.get_wrap(), kit.get_size(), sizes_open);
            ModelRc::new(VecModel::from(items))
        });
        let a = self.clone();
        let w = ui.as_weak();
        kit.on_style(move |wrap, size| {
            let level = editor::font_level(size);
            if let Some(ui) = w.upgrade() {
                let kit = ui.global::<EditorKit>();
                kit.set_wrap(wrap);
                kit.set_size(level as i32);
                kit.set_font_px(editor::FONT_SIZES[level].0);
            }
            a.update_settings(Effect::None, |s| {
                s.editor_line_wrap = wrap;
                s.editor_font_size = level as i32;
            });
        });
        kit.on_complete(|schema, text, caret| shown(completion_at(&schema, &text, caret)));
        kit.on_accept(|key, schema, text, caret, index| {
            let picked = completion_at(&schema, &text, caret).and_then(|(at, c)| {
                let s = c.items.get(usize::try_from(index).ok()?)?;
                Some(complete::accept(&text, at, &c.prefix, s))
            });
            match picked {
                Some((after, caret)) => replaced(&key, &text, after, caret),
                None => EditorEdit::default(),
            }
        });
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
