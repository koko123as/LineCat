//! The filters and scripts as the settings keep them, and the checks their forms make.
//! FlClash: lib/views/config/filters.dart, lib/views/config/scripts.dart.

use serde::{Deserialize, Serialize};

/// `Filter`: a label and the regex it stands for.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Filter {
    pub label: String,
    pub regex: String,
}

/// `Script`: its content lives in `scripts/<id>.js` beside the settings.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Script {
    pub id: String,
    pub label: String,
    pub url: Option<String>,
    /// Milliseconds since the epoch.
    pub updated: u64,
}

/// `_FilterFormState._issue` before the core checks the regex; `at` is the filter being
/// edited, None for a new one.
pub fn filter_issue(
    filters: &[Filter],
    at: Option<usize>,
    label: &str,
    regex: &str,
) -> Option<String> {
    let label = label.trim();
    if label.is_empty() {
        return Some("标签不能为空".into());
    }
    let taken = filters
        .iter()
        .enumerate()
        .any(|(i, f)| Some(i) != at && f.label == label);
    if taken {
        return Some("标签当前已存在".into());
    }
    if regex.trim().is_empty() {
        return Some("正则不能为空".into());
    }
    None
}

/// The name a script is saved under: unique among the others (`existsTip` otherwise).
pub fn script_name_issue(scripts: &[Script], id: Option<&str>, name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        return Some("名称不能为空".into());
    }
    if scripts
        .iter()
        .any(|s| Some(s.id.as_str()) != id && s.label == name)
    {
        return Some("名称当前已存在".into());
    }
    None
}

/// What the script editor's save does with `title` (`_handleEditorSave`).
#[derive(Debug, PartialEq)]
pub enum ScriptSave {
    Unchanged,
    AskName,
    Exists,
    Save(String),
}

/// `_isDirty`: the text or the title differs from what the editor opened with.
pub fn script_dirty(opened: (&str, &str), title: &str, text: &str) -> bool {
    opened != (title, text)
}

pub fn script_save(
    scripts: &[Script],
    id: Option<&str>,
    opened: (&str, &str),
    title: &str,
    text: &str,
) -> ScriptSave {
    if !script_dirty(opened, title, text) {
        return ScriptSave::Unchanged;
    }
    let label = title.trim();
    if label.is_empty() {
        return ScriptSave::AskName;
    }
    match script_name_issue(scripts, id, label) {
        Some(_) => ScriptSave::Exists,
        None => ScriptSave::Save(label.to_string()),
    }
}

/// A name for an imported script: the file name without its extension, made unique.
pub fn import_name(scripts: &[Script], file: &str) -> String {
    let base = file
        .rsplit('/')
        .next()
        .unwrap_or(file)
        .split('?')
        .next()
        .unwrap_or("");
    let base = base.strip_suffix(".js").unwrap_or(base).trim();
    let base = if base.is_empty() { "脚本" } else { base };
    let mut name = base.to_string();
    let mut n = 1;
    while scripts.iter().any(|s| s.label == name) {
        n += 1;
        name = format!("{base} ({n})");
    }
    name
}

/// `_applyIfInUse`: whether the current profile runs the script.
pub fn script_in_use(state: &crate::store::State, id: &str) -> bool {
    let active = state.active.as_deref();
    state
        .profiles
        .iter()
        .any(|p| Some(p.id.as_str()) == active && p.script_id.as_deref() == Some(id))
}

/// `deleteScript`: the profiles running it stop; true when the current one ran it.
pub fn drop_script(state: &mut crate::store::State, id: &str) -> bool {
    let in_use = script_in_use(state, id);
    for p in &mut state.profiles {
        if p.script_id.as_deref() == Some(id) {
            p.script_id = None;
        }
    }
    in_use
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_and_script_checks() {
        let f = vec![Filter {
            label: "香港".into(),
            regex: "HK|香港".into(),
        }];
        assert_eq!(filter_issue(&f, None, " ", "x").unwrap(), "标签不能为空");
        assert_eq!(
            filter_issue(&f, None, "香港", "x").unwrap(),
            "标签当前已存在"
        );
        assert!(filter_issue(&f, Some(0), "香港", "x").is_none());
        assert_eq!(filter_issue(&f, None, "日本", "").unwrap(), "正则不能为空");
        let s = vec![Script {
            id: "1".into(),
            label: "main".into(),
            ..Default::default()
        }];
        assert!(script_name_issue(&s, None, "main").is_some());
        assert!(script_name_issue(&s, Some("1"), "main").is_none());
        assert_eq!(import_name(&s, "https://x.com/a/main.js?v=1"), "main (2)");
        assert_eq!(import_name(&s, ""), "脚本");
    }

    #[test]
    fn the_current_profile_reapplies_its_script() {
        use crate::store::{Profile, State};
        let profile = |id: &str, script: Option<&str>| Profile {
            id: id.into(),
            script_id: script.map(String::from),
            ..Default::default()
        };
        let mut st = State {
            profiles: vec![profile("a", Some("s1")), profile("b", Some("s2"))],
            active: Some("a".into()),
            ..Default::default()
        };
        assert!(script_in_use(&st, "s1"));
        assert!(!script_in_use(&st, "s2"));
        assert!(!drop_script(&mut st, "s2"));
        assert_eq!(st.profiles[1].script_id, None);
        assert!(drop_script(&mut st, "s1"));
        assert_eq!(st.profiles[0].script_id, None);
    }

    #[test]
    fn the_script_editor_saves_by_its_title() {
        let scripts = [
            Script {
                id: "1".into(),
                label: "main".into(),
                ..Default::default()
            },
            Script {
                id: "2".into(),
                label: "other".into(),
                ..Default::default()
            },
        ];
        let opened = ("main", "a");
        assert!(!script_dirty(opened, "main", "a"));
        assert!(script_dirty(opened, "main2", "a"));
        assert!(script_dirty(opened, "main", "b"));
        let save = |id, title, text| script_save(&scripts, id, opened, title, text);
        assert_eq!(save(Some("1"), "main", "a"), ScriptSave::Unchanged);
        assert_eq!(
            save(Some("1"), "main", "b"),
            ScriptSave::Save("main".into())
        );
        assert_eq!(
            save(Some("1"), " new ", "a"),
            ScriptSave::Save("new".into())
        );
        assert_eq!(save(Some("1"), "other", "a"), ScriptSave::Exists);
        assert_eq!(
            script_save(&scripts, None, ("", "t"), "", "u"),
            ScriptSave::AskName
        );
        assert_eq!(
            script_save(&scripts, None, ("", "t"), "main", "t"),
            ScriptSave::Exists
        );
    }
}
