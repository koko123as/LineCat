//! The editor's undo and redo (code_forge's `UndoRedoController`). `TextInput` keeps its
//! own stack only for typing and drops it whenever the text is set from outside, which
//! every replace does, so the page keeps this one instead: typing is grouped into a step
//! at each pause, and each replace is a step of its own.

const DEPTH: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Edit {
    at: usize,
    removed: String,
    inserted: String,
}

fn diff(old: &str, new: &str) -> Option<Edit> {
    if old == new {
        return None;
    }
    let mut prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let room = old.len().min(new.len()) - prefix;
    let mut suffix = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(old.len() - suffix) {
        suffix -= 1;
    }
    Some(Edit {
        at: prefix,
        removed: old[prefix..old.len() - suffix].to_string(),
        inserted: new[prefix..new.len() - suffix].to_string(),
    })
}

fn splice(text: &str, at: usize, len: usize, with: &str) -> String {
    let mut out = String::with_capacity(text.len() - len + with.len());
    out.push_str(&text[..at]);
    out.push_str(with);
    out.push_str(&text[at + len..]);
    out
}

#[derive(Default)]
pub struct History {
    base: String,
    pending: Option<String>,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
}

impl History {
    pub fn new(text: &str) -> Self {
        History {
            base: text.to_string(),
            ..Default::default()
        }
    }

    fn commit(&mut self) {
        let Some(next) = self.pending.take() else {
            return;
        };
        if let Some(edit) = diff(&self.base, &next) {
            self.undo.push(edit);
            if self.undo.len() > DEPTH {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.base = next;
        }
    }

    /// The text after a keystroke; `paused` closes the step typed before it.
    pub fn typed(&mut self, text: &str, paused: bool) {
        if paused {
            self.commit();
        }
        self.pending = Some(text.to_string());
    }

    /// A replace from `before` to `after`, kept apart from the typing around it.
    pub fn replaced(&mut self, before: &str, after: &str) {
        self.pending = Some(before.to_string());
        self.commit();
        self.pending = Some(after.to_string());
        self.commit();
    }

    pub fn can_undo(&self, current: &str) -> bool {
        !self.undo.is_empty() || current != self.base
    }

    pub fn can_redo(&self, current: &str) -> bool {
        !self.redo.is_empty() && current == self.base
    }

    /// The text one step back and the cursor at the end of what came back.
    pub fn undo(&mut self, current: &str) -> Option<(String, usize)> {
        self.pending = Some(current.to_string());
        self.commit();
        let edit = self.undo.pop()?;
        self.base = splice(&self.base, edit.at, edit.inserted.len(), &edit.removed);
        let caret = edit.at + edit.removed.len();
        self.redo.push(edit);
        Some((self.base.clone(), caret))
    }

    pub fn redo(&mut self, current: &str) -> Option<(String, usize)> {
        if current != self.base {
            return None;
        }
        let edit = self.redo.pop()?;
        self.base = splice(&self.base, edit.at, edit.removed.len(), &edit.inserted);
        let caret = edit.at + edit.inserted.len();
        self.undo.push(edit);
        Some((self.base.clone(), caret))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diffs_keep_whole_chars() {
        assert_eq!(diff("abc", "abc"), None);
        assert_eq!(
            diff("mode: rule", "mode: global"),
            Some(Edit {
                at: 6,
                removed: "rule".into(),
                inserted: "global".into(),
            })
        );
        let edit = diff("aé", "aè").unwrap();
        assert_eq!((edit.at, edit.removed.as_str()), (1, "é"));
        let edit = diff("ab", "aab").unwrap();
        assert_eq!((edit.at, edit.inserted.as_str()), (1, "a"));
    }

    #[test]
    fn typing_is_one_step_per_pause() {
        let mut h = History::new("");
        h.typed("a", false);
        h.typed("ab", false);
        h.typed("ab c", true);
        assert!(h.can_undo("ab c"));
        assert_eq!(h.undo("ab c"), Some(("ab".into(), 2)));
        assert_eq!(h.undo("ab"), Some(("".into(), 0)));
        assert_eq!(h.undo(""), None);
        assert!(!h.can_undo(""));
        assert!(h.can_redo(""));
        assert_eq!(h.redo(""), Some(("ab".into(), 2)));
        assert_eq!(h.redo("ab"), Some(("ab c".into(), 4)));
        assert!(!h.can_redo("ab c"));
    }

    #[test]
    fn typing_drops_the_redo() {
        let mut h = History::new("x");
        h.typed("xy", false);
        assert_eq!(h.undo("xy"), Some(("x".into(), 1)));
        assert!(h.can_redo("x"));
        h.typed("xz", false);
        assert!(!h.can_redo("xz"));
        assert_eq!(h.redo("xz"), None);
        assert_eq!(h.undo("xz"), Some(("x".into(), 1)));
        assert_eq!(h.redo("x"), Some(("xz".into(), 2)));
    }

    #[test]
    fn a_replace_is_its_own_step() {
        let mut h = History::new("port: 1");
        h.typed("port: 12", false);
        h.replaced("port: 12", "mixed-port: 12");
        assert_eq!(h.undo("mixed-port: 12"), Some(("port: 12".into(), 0)));
        assert_eq!(h.undo("port: 12"), Some(("port: 1".into(), 7)));
    }

    #[test]
    fn keeps_a_bounded_depth() {
        let mut h = History::new("");
        let mut text = String::new();
        for _ in 0..DEPTH + 20 {
            text.push('a');
            h.typed(&text, true);
        }
        let mut steps = 0;
        while let Some((t, _)) = h.undo(&text) {
            text = t;
            steps += 1;
        }
        assert_eq!(steps, DEPTH);
    }
}
