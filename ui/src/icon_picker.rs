//! The group form's icon picker (FlClash: lib/views/profiles/custom/icon.dart, `IconPickerView`):
//! icons recommended for the group's name from the icon sets and the icons used before
//! (`IconRecords`), the sets one at a time, the search across them, and the current icon.

use crate::icon_sets::{url_file_name, IconSet, IconSetIcon};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const RECOMMENDED: &str = "recommended";
const RECORD_CAPACITY: usize = 1000;
const RANK_LIMIT: usize = 24;

/// A row of `icon_records`: an icon a saved group used, by when it was last saved.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct IconRecord {
    pub url: String,
    pub last_accessed_ms: u64,
}

/// `IconRecordsDao.put`: the oldest go past the capacity.
pub fn put_record(records: &mut Vec<IconRecord>, url: &str, now: u64) {
    records.retain(|r| r.url != url);
    records.push(IconRecord {
        url: url.to_string(),
        last_accessed_ms: now,
    });
    if records.len() > RECORD_CAPACITY {
        records.sort_by_key(|r| std::cmp::Reverse(r.last_accessed_ms));
        records.truncate(RECORD_CAPACITY);
    }
}

/// `IconRecordsDao.query('')`: the latest first.
pub fn recent(records: &[IconRecord]) -> Vec<IconSetIcon> {
    let mut sorted: Vec<&IconRecord> = records.iter().collect();
    sorted.sort_by_key(|r| std::cmp::Reverse(r.last_accessed_ms));
    sorted.into_iter().map(|r| record_icon(&r.url)).collect()
}

/// `_recordIcon`: a data url has no name.
pub fn record_icon(url: &str) -> IconSetIcon {
    IconSetIcon {
        name: if url.starts_with("data:") {
            String::new()
        } else {
            url_file_name(url)
        },
        url: url.to_string(),
    }
}

pub fn label(icon: &IconSetIcon) -> String {
    if icon.name.is_empty() {
        "本地图片".into()
    } else {
        icon.label()
    }
}

fn compact(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NameMatch {
    pub index: usize,
    pub score: u8,
    pub length: usize,
}

/// `matchIconNames`, English only: a name with no Latin letters or digits matches nothing.
pub fn match_names(names: &[&str], target: &str) -> Vec<NameMatch> {
    let target = compact(target);
    if target.is_empty() {
        return Vec::new();
    }
    names
        .iter()
        .enumerate()
        .filter_map(|(index, n)| {
            let c = compact(&crate::rule_sets::file_stem(n));
            if c.is_empty() {
                return None;
            }
            let score = if c == target {
                3
            } else if c.len() >= 3 && target.contains(&c) {
                2
            } else if target.len() >= 3 && c.contains(&target) {
                1
            } else {
                0
            };
            (score > 0).then_some(NameMatch {
                index,
                score,
                length: c.len(),
            })
        })
        .collect()
}

fn score_for(icons: &[IconSetIcon], target: &str) -> Vec<(IconSetIcon, NameMatch)> {
    let names: Vec<&str> = icons.iter().map(|i| i.name.as_str()).collect();
    match_names(&names, target)
        .into_iter()
        .map(|m| (icons[m.index].clone(), m))
        .collect()
}

/// `rankIconMatches`: each picture's first match, then by score and the shorter name.
pub fn rank(matches: Vec<(IconSetIcon, NameMatch)>) -> Vec<IconSetIcon> {
    let mut seen = HashSet::new();
    let mut scored: Vec<(IconSetIcon, NameMatch)> = matches
        .into_iter()
        .filter(|(i, _)| seen.insert(i.url.clone()))
        .collect();
    scored.sort_by(|a, b| b.1.score.cmp(&a.1.score).then(a.1.length.cmp(&b.1.length)));
    scored
        .into_iter()
        .take(RANK_LIMIT)
        .map(|(i, _)| i)
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub key: String,
    pub label: String,
    pub icons: Vec<IconSetIcon>,
}

/// `_sourcesOf`: 推荐 (ranked matches, then the other recent icons), then each set with icons.
pub fn sources(sets: &[IconSet], recent: &[IconSetIcon], group_name: &str) -> Vec<Source> {
    let all: Vec<IconSetIcon> = sets.iter().flat_map(|s| s.icons.clone()).collect();
    let mut matches = score_for(&all, group_name);
    matches.extend(score_for(recent, group_name));
    let mut suggested = rank(matches);
    let mut urls: HashSet<String> = suggested.iter().map(|i| i.url.clone()).collect();
    suggested.extend(
        recent
            .iter()
            .filter(|i| urls.insert(i.url.clone()))
            .cloned(),
    );
    let mut out = Vec::new();
    if !suggested.is_empty() {
        out.push(Source {
            key: RECOMMENDED.into(),
            label: "推荐".into(),
            icons: suggested,
        });
    }
    out.extend(sets.iter().filter(|s| !s.icons.is_empty()).map(|s| Source {
        key: s.id.clone(),
        label: s.name.clone(),
        icons: s.icons.clone(),
    }));
    out
}

/// `_initialSourceKey`: the set holding the current icon, else the first source.
pub fn initial_source(sources: &[Source], value: &str) -> Option<String> {
    if !value.is_empty() {
        if let Some(s) = sources
            .iter()
            .find(|s| s.key != RECOMMENDED && s.icons.iter().any(|i| i.url == value))
        {
            return Some(s.key.clone());
        }
    }
    sources.first().map(|s| s.key.clone())
}

pub fn shown_source(sources: &[Source], picked: Option<&str>, value: &str) -> Option<String> {
    match picked {
        Some(k) if sources.iter().any(|s| s.key == k) => Some(k.to_string()),
        _ => initial_source(sources, value),
    }
}

/// `_buildSearchSlivers`: the sources' icons the search leaves, each picture once.
pub fn search(sources: &[Source], query: &str) -> Vec<Source> {
    let mut shown = HashSet::new();
    sources
        .iter()
        .filter_map(|s| {
            let icons: Vec<IconSetIcon> = s
                .icons
                .iter()
                .filter(|i| crate::proxies::matches(query, &label(i)))
                .filter(|i| shown.insert(i.url.clone()))
                .cloned()
                .collect();
            (!icons.is_empty()).then(|| Source { icons, ..s.clone() })
        })
        .collect()
}

pub fn describe(src: &str, sets: &[IconSet]) -> (String, String) {
    for s in sets {
        if let Some(i) = s.icons.iter().find(|i| i.url == src) {
            return (i.label(), s.name.clone());
        }
    }
    if src.starts_with("data:") {
        return ("本地图片".into(), String::new());
    }
    let stem = crate::rule_sets::file_stem(&url_file_name(src));
    let host = crate::store::host_of(src).unwrap_or_default();
    (
        if stem.is_empty() {
            src.to_string()
        } else {
            stem
        },
        host,
    )
}

/// `_handleLink`'s validator.
pub fn url_issue(value: &str) -> Option<&'static str> {
    let v = value.trim();
    if v.is_empty() {
        Some("URL不能为空")
    } else if !crate::icon_sets::is_http_url(v) {
        Some("URL必须为URL")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icon(name: &str, url: &str) -> IconSetIcon {
        IconSetIcon {
            name: name.into(),
            url: url.into(),
        }
    }

    fn set(id: &str, icons: Vec<IconSetIcon>) -> IconSet {
        IconSet {
            id: id.into(),
            name: format!("set {id}"),
            icons,
            ..Default::default()
        }
    }

    #[test]
    fn names_match_by_their_compact_stems() {
        let names = [
            "Hong_Kong.png",
            "HK.png",
            "Netflix.png",
            "港.png",
            "Telegram_X.png",
        ];
        let m = match_names(&names, "🇭🇰 Hong Kong");
        assert_eq!(m.len(), 1);
        assert_eq!((m[0].index, m[0].score), (0, 3));
        let m = match_names(&names, "Netflix 港");
        assert_eq!((m[0].index, m[0].score), (2, 3));
        let m = match_names(&names, "Telegram");
        assert_eq!((m[0].index, m[0].score), (4, 1));
        let m = match_names(&names, "Netflix Plus");
        assert_eq!((m[0].index, m[0].score), (2, 2));
        assert!(match_names(&names, "香港").is_empty());
    }

    #[test]
    fn recommendations_rank_then_follow_with_recent_icons() {
        let sets = vec![
            set(
                "1",
                vec![
                    icon("Netflix_Letter.png", "https://h/nl.png"),
                    icon("Netflix.png", "https://h/n.png"),
                    icon("Apple.png", "https://h/a.png"),
                ],
            ),
            set("2", vec![]),
        ];
        let recent = vec![
            record_icon("https://h/n.png"),
            record_icon("data:image/png;base64,AA"),
        ];
        let s = sources(&sets, &recent, "Netflix");
        assert_eq!(
            s.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(),
            [RECOMMENDED, "1"]
        );
        let urls: Vec<&str> = s[0].icons.iter().map(|i| i.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                "https://h/n.png",
                "https://h/nl.png",
                "data:image/png;base64,AA"
            ]
        );
        assert_eq!(label(&s[0].icons[2]), "本地图片");
        assert_eq!(initial_source(&s, "https://h/a.png").as_deref(), Some("1"));
        assert_eq!(initial_source(&s, "").as_deref(), Some(RECOMMENDED));
        assert_eq!(
            shown_source(&s, Some("gone"), "").as_deref(),
            Some(RECOMMENDED)
        );
        let found = search(&s, "netflix");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].icons.len(), 2);
        assert!(sources(&[], &[], "x").is_empty());
    }

    #[test]
    fn records_keep_the_latest_use_once() {
        let mut r = Vec::new();
        put_record(&mut r, "https://h/a.png", 1);
        put_record(&mut r, "https://h/b.png", 2);
        put_record(&mut r, "https://h/a.png", 3);
        assert_eq!(r.len(), 2);
        let urls: Vec<String> = recent(&r).into_iter().map(|i| i.url).collect();
        assert_eq!(urls, ["https://h/a.png", "https://h/b.png"]);
        for i in 0..RECORD_CAPACITY as u64 {
            put_record(&mut r, &format!("https://h/{i}.png"), 10 + i);
        }
        assert_eq!(r.len(), RECORD_CAPACITY);
        assert!(!r.iter().any(|x| x.url == "https://h/b.png"));
    }

    #[test]
    fn the_current_icon_describes_itself() {
        let sets = vec![set("1", vec![icon("Hong_Kong.png", "https://h/hk.png")])];
        assert_eq!(
            describe("https://h/hk.png", &sets),
            ("Hong Kong".into(), "set 1".into())
        );
        assert_eq!(describe("data:image/png;base64,AA", &sets).0, "本地图片");
        assert_eq!(
            describe("https://cdn.example/x/Proxy.png?v=1", &sets),
            ("Proxy".into(), "cdn.example".into())
        );
        assert_eq!(url_issue(" "), Some("URL不能为空"));
        assert_eq!(url_issue("ftp://x"), Some("URL必须为URL"));
        assert_eq!(url_issue("https://x/a.png"), None);
    }
}
