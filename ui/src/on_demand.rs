//! On-demand rules and the suspend they drive (`suspendProvider`, `syncSuspend`).
//! FlClash: lib/views/config/on_demand.dart.

use std::collections::BTreeSet;

pub const SSID_MAX: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Permission {
    Granted,
    #[default]
    Denied,
    PermanentlyDenied,
}

impl Permission {
    pub fn decode(value: i64) -> Self {
        match value {
            0 => Permission::Granted,
            2 => Permission::PermanentlyDenied,
            _ => Permission::Denied,
        }
    }

    /// `LocationPermissions.refresh`: a check keeps a permanent denial until the grant.
    pub fn after_check(self, checked: Permission) -> Permission {
        if checked == Permission::Granted || self != Permission::PermanentlyDenied {
            checked
        } else {
            self
        }
    }
}

pub fn suspended(ssid: Option<&str>, excluded: &[String]) -> bool {
    ssid.is_some_and(|s| excluded.iter().any(|e| e == s))
}

/// `_updateSsid`: read only on Wi-Fi once a network is excluded.
pub fn reads_ssid(on_wifi: bool, excluded: &[String]) -> bool {
    on_wifi && !excluded.is_empty()
}

/// The run state the app shows: a run parked by an excluded SSID still reads as started.
pub fn shown_state(service: &str, parked: bool) -> &str {
    if parked {
        "STARTED"
    } else {
        service
    }
}

pub fn ssid_issue(list: &[String], editing: Option<&str>, value: &str) -> Option<String> {
    let value = clip(value);
    if value.is_empty() {
        return Some("SSID不能为空".into());
    }
    if list.iter().any(|s| *s == value) && editing != Some(value.as_str()) {
        return Some("SSID当前已存在".into());
    }
    None
}

pub fn clip(value: &str) -> String {
    value.chars().take(SSID_MAX).collect()
}

/// `_handleAddOrUpdate`: the edited one leaves its place, and the new one goes last.
pub fn put(list: &[String], old: Option<&str>, new: &str) -> Vec<String> {
    let new = clip(new);
    let mut out: Vec<String> = Vec::new();
    for s in list {
        if Some(s.as_str()) != old && *s != new && !out.contains(s) {
            out.push(s.clone());
        }
    }
    out.push(new);
    out
}

pub fn select_all(selected: &BTreeSet<String>, list: &[String]) -> BTreeSet<String> {
    if list.iter().all(|s| selected.contains(s)) {
        BTreeSet::new()
    } else {
        list.iter().cloned().collect()
    }
}

pub fn without(list: &[String], selected: &BTreeSet<String>) -> Vec<String> {
    list.iter()
        .filter(|s| !selected.contains(*s))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn decodes_and_keeps_a_permanent_denial() {
        assert_eq!(Permission::decode(0), Permission::Granted);
        assert_eq!(Permission::decode(1), Permission::Denied);
        assert_eq!(Permission::decode(2), Permission::PermanentlyDenied);
        assert_eq!(Permission::decode(7), Permission::Denied);
        let p = Permission::PermanentlyDenied;
        assert_eq!(p.after_check(Permission::Denied), p);
        assert_eq!(p.after_check(Permission::Granted), Permission::Granted);
        assert_eq!(
            Permission::Granted.after_check(Permission::Denied),
            Permission::Denied
        );
    }

    #[test]
    fn suspends_on_an_excluded_wifi_only() {
        let list = v(&["Home", "Office"]);
        assert!(suspended(Some("Home"), &list));
        assert!(!suspended(Some("Cafe"), &list));
        assert!(!suspended(None, &list));
        assert!(!reads_ssid(true, &[]));
        assert!(!reads_ssid(false, &list));
        assert!(reads_ssid(true, &list));
        assert_eq!(shown_state("STOPPED", true), "STARTED");
        assert_eq!(shown_state("STOPPING", false), "STOPPING");
    }

    #[test]
    fn checks_and_puts_ssids() {
        let list = v(&["Home", "Office"]);
        assert_eq!(ssid_issue(&list, None, "").as_deref(), Some("SSID不能为空"));
        assert_eq!(
            ssid_issue(&list, None, "Home").as_deref(),
            Some("SSID当前已存在")
        );
        assert_eq!(ssid_issue(&list, Some("Home"), "Home"), None);
        assert_eq!(put(&list, None, "Cafe"), v(&["Home", "Office", "Cafe"]));
        assert_eq!(put(&list, Some("Home"), "Lab"), v(&["Office", "Lab"]));
        assert_eq!(clip(&"x".repeat(40)).len(), SSID_MAX);
    }

    #[test]
    fn selects_all_then_none_and_deletes() {
        let list = v(&["a", "b"]);
        let one: BTreeSet<String> = ["a".to_string()].into();
        let all = select_all(&one, &list);
        assert_eq!(all.len(), 2);
        assert!(select_all(&all, &list).is_empty());
        assert_eq!(without(&list, &one), v(&["b"]));
    }
}
