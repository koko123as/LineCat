//! The text editor's find and replace (FlClash: lib/pages/editor.dart, code_forge's
//! `FindController`), on the UTF-8 byte offsets `TextInput` keeps its cursor in.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub start: usize,
    pub end: usize,
    pub index: usize,
    pub count: usize,
}

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Left to right, not overlapping.
pub fn find_all(text: &str, query: &str, match_case: bool) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    if match_case {
        return text
            .match_indices(query)
            .map(|(i, m)| (i, i + m.len()))
            .collect();
    }
    let wanted: Vec<char> = query.chars().map(fold).collect();
    let mut out = Vec::new();
    let mut next = 0;
    for (start, c) in text.char_indices() {
        if start < next || fold(c) != wanted[0] {
            continue;
        }
        let mut rest = text[start..].char_indices();
        let mut end = start;
        let found = wanted.iter().all(|&w| match rest.next() {
            Some((at, t)) if fold(t) == w => {
                end = start + at + t.len_utf8();
                true
            }
            _ => false,
        });
        if found {
            out.push((start, end));
            next = end;
        }
    }
    out
}

/// The first match starting at or after `from`, or with `back` the last one starting
/// before it; past either end the search wraps around.
pub fn find(text: &str, query: &str, match_case: bool, from: usize, back: bool) -> Option<Hit> {
    let all = find_all(text, query, match_case);
    let index = if back {
        all.iter()
            .rposition(|&(s, _)| s < from)
            .unwrap_or(all.len().checked_sub(1)?)
    } else {
        all.iter().position(|&(s, _)| s >= from).unwrap_or(0)
    };
    let &(start, end) = all.get(index)?;
    Some(Hit {
        start,
        end,
        index,
        count: all.len(),
    })
}

/// `FindController.replace` of the match starting at `at`, with the offset after it.
pub fn replace_at(
    text: &str,
    query: &str,
    match_case: bool,
    at: usize,
    with: &str,
) -> Option<(String, usize)> {
    let (start, end) = find_all(text, query, match_case)
        .into_iter()
        .find(|&(s, _)| s == at)?;
    let mut out = String::with_capacity(text.len() - (end - start) + with.len());
    out.push_str(&text[..start]);
    out.push_str(with);
    out.push_str(&text[end..]);
    Some((out, start + with.len()))
}

/// `FindController.replaceAll`, with how many were replaced.
pub fn replace_all(text: &str, query: &str, match_case: bool, with: &str) -> (String, usize) {
    let all = find_all(text, query, match_case);
    if all.is_empty() {
        return (text.to_string(), 0);
    }
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for &(start, end) in &all {
        out.push_str(&text[last..start]);
        out.push_str(with);
        last = end;
    }
    out.push_str(&text[last..]);
    (out, all.len())
}

/// The find field's count: empty before anything is typed, 无 with no match, else n/total.
pub fn hit_label(query: &str, hit: Option<Hit>) -> String {
    match hit {
        _ if query.is_empty() => String::new(),
        None => "无".into(),
        Some(h) => format!("{}/{}", h.index + 1, h.count),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_with_and_without_case() {
        let text = "mode: rule\nMode: Rule\nrule-x";
        assert_eq!(find_all(text, "rule", true), vec![(6, 10), (22, 26)]);
        assert_eq!(
            find_all(text, "RULE", false),
            vec![(6, 10), (17, 21), (22, 26)]
        );
        assert_eq!(find_all(text, "", false), vec![]);
        assert_eq!(find_all("aaaa", "aa", true), vec![(0, 2), (2, 4)]);
        assert_eq!(find_all("aaaa", "AA", false), vec![(0, 2), (2, 4)]);
    }

    #[test]
    fn folds_beyond_ascii() {
        let text = "ÄBC äbc";
        assert_eq!(find_all(text, "äb", false), vec![(0, 3), (5, 8)]);
        assert_eq!(find_all(text, "äb", true), vec![(5, 8)]);
    }

    #[test]
    fn steps_and_wraps() {
        let text = "a1 a2 a3";
        let at = |from, back| find(text, "a", true, from, back).map(|h| (h.start, h.index));
        assert_eq!(at(0, false), Some((0, 0)));
        assert_eq!(at(1, false), Some((3, 1)));
        assert_eq!(at(7, false), Some((0, 0)));
        assert_eq!(at(3, true), Some((0, 0)));
        assert_eq!(at(0, true), Some((6, 2)));
        assert_eq!(find(text, "z", true, 0, false), None);
        assert_eq!(find(text, "z", true, 0, true), None);
        assert_eq!(find(text, "a", true, 0, false).unwrap().count, 3);
    }

    #[test]
    fn replaces_one_and_all() {
        let text = "port: 1\nPort: 2";
        assert_eq!(
            replace_at(text, "port", false, 8, "mixed-port"),
            Some(("port: 1\nmixed-port: 2".to_string(), 18))
        );
        assert_eq!(replace_at(text, "port", false, 3, "x"), None);
        assert_eq!(
            replace_all(text, "port", false, "p"),
            ("p: 1\np: 2".to_string(), 2)
        );
        assert_eq!(
            replace_all(text, "port", true, "p"),
            ("p: 1\nPort: 2".to_string(), 1)
        );
        assert_eq!(replace_all(text, "zz", true, "p").1, 0);
    }

    #[test]
    fn labels_the_count() {
        assert_eq!(hit_label("", None), "");
        assert_eq!(hit_label("x", None), "无");
        let hit = Hit {
            start: 0,
            end: 1,
            index: 1,
            count: 4,
        };
        assert_eq!(hit_label("x", Some(hit)), "2/4");
    }
}
