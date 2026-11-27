//! The font families the font page lists (plugins/rust_api's `fonts`) and the text scale
//! the theme page sets (`_TextScaleFactorItem`).

use std::collections::HashSet;

pub const MIN_SCALE: f32 = 0.8;
pub const MAX_SCALE: f32 = 1.4;
pub const STEP: f32 = 0.05;

/// Android's families that can be picked by name, from its fonts.xml; none elsewhere.
pub fn system_families() -> Vec<String> {
    #[cfg(target_os = "android")]
    let families = std::fs::read_to_string("/system/etc/fonts.xml")
        .map(|xml| named_families(&xml))
        .unwrap_or_default();
    #[cfg(not(target_os = "android"))]
    let families = Vec::<String>::new();
    sort_families(families)
}

/// Case-insensitive and in UTF-16 order; hidden families start with a dot.
pub fn sort_families(families: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique: Vec<(String, String)> = families
        .into_iter()
        .filter_map(|family| {
            let name = family.trim();
            if name.is_empty() || name.starts_with('.') {
                return None;
            }
            let key = name.to_lowercase();
            seen.insert(key.clone()).then(|| (key, name.to_owned()))
        })
        .collect();
    unique.sort_by(|(a, _), (b, _)| a.encode_utf16().cmp(b.encode_utf16()));
    unique.into_iter().map(|(_, name)| name).collect()
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The `name` of every `<family>`; an unnamed family is a language's fallback.
pub fn named_families(xml: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<family") {
        rest = &rest[start + "<family".len()..];
        if rest.bytes().next().is_none_or(is_word) {
            continue;
        }
        let tag = &rest[..rest.find('>').unwrap_or(rest.len())];
        if let Some(name) = name_attribute(tag) {
            names.push(name.to_owned());
        }
    }
    names
}

fn name_attribute(tag: &str) -> Option<&str> {
    let bytes = tag.as_bytes();
    let mut from = 0;
    while let Some(at) = tag[from..].find("name=\"") {
        let at = from + at;
        let value = at + "name=\"".len();
        if at == 0 || !is_word(bytes[at - 1]) {
            let end = tag[value..].find('"')?;
            return (end > 0).then(|| &tag[value..value + end]);
        }
        from = value;
    }
    None
}

/// The scale text is drawn at: the chosen one, or the system's within the slider's range.
pub fn effective_scale(enable: bool, scale: f32, system: f32) -> f32 {
    if enable {
        scale.clamp(MIN_SCALE, MAX_SCALE)
    } else {
        system.clamp(MIN_SCALE, MAX_SCALE)
    }
}

/// A slider value on its 5% divisions.
pub fn snap(value: f32) -> f32 {
    let steps = ((value.clamp(MIN_SCALE, MAX_SCALE) - MIN_SCALE) / STEP).round();
    ((MIN_SCALE + steps * STEP) * 100.0).round() / 100.0
}

/// The families whose names hold `query`, ignoring case.
pub fn matching(families: &[String], query: &str) -> Vec<String> {
    let query = query.trim().to_lowercase();
    families
        .iter()
        .filter(|f| query.is_empty() || f.to_lowercase().contains(&query))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_the_named_families_of_a_fonts_xml() {
        let xml = r#"
<familyset version="23">
  <family name="sans-serif" varied="true">
    <font weight="400" style="normal">Roboto-Regular.ttf</font>
  </family>
  <alias name="arial" to="sans-serif" />
  <family lang="zh-Hans">
    <font weight="400" style="normal" index="2">NotoSansCJK-Regular.ttc</font>
  </family>
  <family
      name="serif-monospace">
    <font weight="400" style="normal">CutiveMono.ttf</font>
  </family>
  <family fallbackFor="serif" xname="no">
  </family>
</familyset>
"#;
        assert_eq!(named_families(xml), ["sans-serif", "serif-monospace"]);
    }

    #[test]
    fn sorts_without_case_and_drops_duplicates_and_hidden_ones() {
        let families = [
            "Segoe UI",
            " arial ",
            ".AppleSystemUIFont",
            "",
            "segoe ui",
            "Arial",
            "Microsoft YaHei",
        ]
        .map(String::from);
        assert_eq!(
            sort_families(families),
            ["arial", "Microsoft YaHei", "Segoe UI"]
        );
    }

    #[test]
    fn custom_scale_wins_and_the_system_one_is_clamped() {
        assert_eq!(effective_scale(true, 1.25, 2.0), 1.25);
        assert_eq!(effective_scale(false, 1.25, 2.0), MAX_SCALE);
        assert_eq!(effective_scale(false, 1.25, 0.5), MIN_SCALE);
        assert_eq!(effective_scale(false, 1.25, 1.1), 1.1);
    }

    #[test]
    fn slider_values_snap_to_five_percent() {
        assert_eq!(snap(1.012), 1.0);
        assert_eq!(snap(1.074), 1.05);
        assert_eq!(snap(1.39), 1.4);
        assert_eq!(snap(3.0), 1.4);
    }

    #[test]
    fn search_ignores_case() {
        let families = ["sans-serif", "Serif", "monospace"].map(String::from);
        assert_eq!(matching(&families, " SER"), ["sans-serif", "Serif"]);
        assert_eq!(matching(&families, "").len(), 3);
    }
}
