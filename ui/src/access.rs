//! Per-app proxying: the saved choice, and the list the page shows from the installed apps.
//! FlClash: lib/views/access.dart, lib/models (`AccessControlProps`, `PackagesExt`).

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// `AccessControlMode`; serialized as the Kotlin shell's `VpnOptions` reads it.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Mode {
    #[serde(rename = "acceptSelected")]
    Accept,
    #[default]
    #[serde(rename = "rejectSelected")]
    Reject,
}

/// `AccessSortType`.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Sort {
    #[default]
    None,
    Name,
    Time,
}

/// `AccessControlProps`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct AccessControl {
    pub enable: bool,
    pub mode: Mode,
    pub accept_list: Vec<String>,
    pub reject_list: Vec<String>,
    pub sort: Sort,
    pub is_filter_system_app: bool,
    pub is_filter_non_internet_app: bool,
}

impl Default for AccessControl {
    fn default() -> Self {
        AccessControl {
            enable: false,
            mode: Mode::Reject,
            accept_list: Vec::new(),
            reject_list: Vec::new(),
            sort: Sort::None,
            is_filter_system_app: true,
            is_filter_non_internet_app: true,
        }
    }
}

impl AccessControl {
    pub fn current(&self) -> &Vec<String> {
        match self.mode {
            Mode::Accept => &self.accept_list,
            Mode::Reject => &self.reject_list,
        }
    }

    pub fn current_mut(&mut self) -> &mut Vec<String> {
        match self.mode {
            Mode::Accept => &mut self.accept_list,
            Mode::Reject => &mut self.reject_list,
        }
    }

    /// `_withSortedList`: the lists compared and saved sorted.
    pub fn sorted(&self) -> AccessControl {
        let mut s = self.clone();
        s.accept_list.sort();
        s.reject_list.sort();
        s
    }

    /// `_handleSelected`: in the current list or out of it.
    pub fn toggle(&mut self, name: &str) {
        let list = self.current_mut();
        if let Some(i) = list.iter().position(|n| n == name) {
            list.remove(i);
        } else {
            list.push(name.to_string());
        }
    }

    /// `_buildSelectAllAction`: all of `names` leave the list when it holds them all, else
    /// they all join it.
    pub fn select_all(&mut self, names: &[String]) {
        let all = self.holds_all(names);
        let list = self.current_mut();
        if all {
            list.retain(|n| !names.contains(n));
        } else {
            for n in names {
                if !list.contains(n) {
                    list.push(n.clone());
                }
            }
        }
    }

    pub fn holds_all(&self, names: &[String]) -> bool {
        let set: HashSet<&String> = self.current().iter().collect();
        !names.is_empty() && names.iter().all(|n| set.contains(n))
    }

    /// `_intelligentSelected`: of the visible apps, the Chinese ones bypass (reject list) and
    /// the rest go through (accept list); the hidden ones keep their place.
    pub fn intelligent(&mut self, packages: &[Package], china: &[String]) {
        let china: HashSet<&String> = china.iter().collect();
        let visible: Vec<&String> = packages
            .iter()
            .filter(|p| self.visible(p))
            .map(|p| &p.package_name)
            .collect();
        let shown: HashSet<&String> = visible.iter().copied().collect();
        let with = |list: &Vec<String>, keep: &dyn Fn(&String) -> bool| -> Vec<String> {
            let mut out: Vec<String> = list
                .iter()
                .filter(|n| !shown.contains(n))
                .cloned()
                .collect();
            out.extend(visible.iter().filter(|n| keep(n)).map(|n| (*n).clone()));
            out
        };
        let accept = with(&self.accept_list, &|n| !china.contains(n));
        let reject = with(&self.reject_list, &|n| china.contains(n));
        self.accept_list = accept;
        self.reject_list = reject;
    }

    /// `whereVisible`.
    pub fn visible(&self, p: &Package) -> bool {
        (!self.is_filter_system_app || !p.system)
            && (!self.is_filter_non_internet_app || p.internet)
    }

    /// `getViewList`: the visible apps, the pinned ones (those selected when the page
    /// opened) first, each part in the chosen order.
    pub fn view<'a>(&self, packages: &'a [Package], pinned: &[String]) -> Vec<&'a Package> {
        let pinned: HashSet<&String> = pinned.iter().collect();
        let (mut first, mut rest): (Vec<&Package>, Vec<&Package>) = packages
            .iter()
            .filter(|p| self.visible(p))
            .partition(|p| pinned.contains(&p.package_name));
        let order = |v: &mut Vec<&Package>| match self.sort {
            Sort::None => {}
            Sort::Name => v.sort_by(|a, b| a.label.cmp(&b.label)),
            Sort::Time => v.sort_by_key(|p| std::cmp::Reverse(p.last_update_time)),
        };
        order(&mut first);
        order(&mut rest);
        first.extend(rest);
        first
    }
}

/// `Package`, as the shell's `packages.list` sends it.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Package {
    pub package_name: String,
    pub label: String,
    pub system: bool,
    pub internet: bool,
    pub last_update_time: i64,
}

/// `SearchQuery.matches` on label and package name: every word, ignoring case.
pub fn matches(p: &Package, query: &str) -> bool {
    let label = p.label.to_lowercase();
    let name = p.package_name.to_lowercase();
    query.split_whitespace().all(|w| {
        let w = w.to_lowercase();
        label.contains(&w) || name.contains(&w)
    })
}

/// `_packageNamePattern`: `^[a-zA-Z]\w*(\.[a-zA-Z]\w*)+$`.
fn is_package_name(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() >= 2
        && parts.iter().all(|p| {
            let mut c = p.chars();
            c.next().is_some_and(|f| f.is_ascii_alphabetic())
                && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        })
}

/// `_parsePackageNames`: one per line, deduplicated in order.
pub fn parse_package_names(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.split('\n').map(str::trim) {
        if is_package_name(line) && !out.iter().any(|n| n == line) {
            out.push(line.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, label: &str, system: bool, internet: bool, t: i64) -> Package {
        Package {
            package_name: name.into(),
            label: label.into(),
            system,
            internet,
            last_update_time: t,
        }
    }

    fn sample() -> Vec<Package> {
        vec![
            pkg("com.b", "Beta", false, true, 2),
            pkg("com.a", "Alpha", false, true, 1),
            pkg("android.sys", "Sys", true, true, 3),
            pkg("com.offline", "Offline", false, false, 4),
            pkg("com.tencent.mm", "微信", false, true, 5),
        ]
    }

    #[test]
    fn view_filters_pins_and_sorts() {
        let mut ac = AccessControl::default();
        let p = sample();
        let names = |v: Vec<&Package>| v.iter().map(|p| p.package_name.clone()).collect::<Vec<_>>();
        assert_eq!(
            names(ac.view(&p, &[])),
            ["com.b", "com.a", "com.tencent.mm"]
        );
        ac.sort = Sort::Name;
        assert_eq!(
            names(ac.view(&p, &["com.tencent.mm".into()])),
            ["com.tencent.mm", "com.a", "com.b"]
        );
        ac.sort = Sort::Time;
        ac.is_filter_system_app = false;
        ac.is_filter_non_internet_app = false;
        assert_eq!(
            names(ac.view(&p, &[])),
            [
                "com.tencent.mm",
                "com.offline",
                "android.sys",
                "com.b",
                "com.a"
            ]
        );
    }

    #[test]
    fn toggle_and_select_all_follow_the_mode() {
        let mut ac = AccessControl::default();
        ac.toggle("com.a");
        assert_eq!(ac.reject_list, ["com.a"]);
        ac.mode = Mode::Accept;
        let all = vec!["com.a".to_string(), "com.b".to_string()];
        ac.select_all(&all);
        assert_eq!(ac.accept_list, all);
        assert!(ac.holds_all(&all));
        ac.select_all(&all);
        assert!(ac.accept_list.is_empty());
        assert_eq!(ac.reject_list, ["com.a"]);
        ac.toggle("com.a");
        ac.toggle("com.a");
        assert!(ac.accept_list.is_empty());
        assert!(!ac.holds_all(&[]));
    }

    #[test]
    fn intelligent_splits_visible_apps() {
        let mut ac = AccessControl {
            accept_list: vec!["android.sys".into(), "com.a".into()],
            ..Default::default()
        };
        ac.intelligent(&sample(), &["com.tencent.mm".into()]);
        assert_eq!(ac.accept_list, ["android.sys", "com.b", "com.a"]);
        assert_eq!(ac.reject_list, ["com.tencent.mm"]);
    }

    #[test]
    fn parses_package_names() {
        let text = "com.a\n  com.b_2 \nnot a name\ncom\n1com.x\ncom.a\ncom..x\norg.x.y";
        assert_eq!(parse_package_names(text), ["com.a", "com.b_2", "org.x.y"]);
    }

    #[test]
    fn serializes_as_the_shell_reads_it() {
        let v = serde_json::to_value(AccessControl::default()).unwrap();
        assert_eq!(v["mode"], "rejectSelected");
        assert_eq!(v["acceptList"], serde_json::json!([]));
        assert_eq!(v["isFilterSystemApp"], true);
        assert!(matches(&sample()[4], "TENCENT"));
        assert!(matches(&sample()[4], "微信"));
        assert!(!matches(&sample()[0], "alpha"));
    }
}
