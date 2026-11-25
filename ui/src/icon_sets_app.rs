//! 图标集 on `App`'s side (lib/views/config/icon_sets.dart and `IconSetsAction`).

use crate::icon_sets::{self, IconSet, IconSetIcon};
use crate::{
    net_images, ActionItem, Anchor, App, IconTile, NetImages, PageMeta, SetRow, SetSection,
};
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// The set the preview is open on and its search.
fn preview() -> &'static Mutex<(String, String)> {
    static P: OnceLock<Mutex<(String, String)>> = OnceLock::new();
    P.get_or_init(Default::default)
}

static FETCHING: AtomicBool = AtomicBool::new(false);

pub fn page_meta(sets: &[IconSet], page: &str) -> Option<PageMeta> {
    (page == "icon-sets").then(|| PageMeta {
        known: true,
        title: "图标集".into(),
        parent: "advanced".into(),
        action: "add-circle".into(),
        action2: if sets.len() > 1 {
            "sort".into()
        } else {
            "".into()
        },
        empty: "暂无图标集".into(),
        long_press: true,
        ..Default::default()
    })
}

pub fn sections(sets: &[IconSet], page: &str) -> Option<Vec<SetSection>> {
    if page != "icon-sets" {
        return None;
    }
    if sets.is_empty() {
        return Some(Vec::new());
    }
    let rows: Vec<SetRow> = sets
        .iter()
        .map(|s| SetRow {
            id: format!("is:{}", s.id).into(),
            kind: "open".into(),
            title: s.name.as_str().into(),
            subtitle: if s.is_remote() { "远程" } else { "本地" }.into(),
            chip: icon_sets::compact(s.icons.len()).into(),
            image: s.cover().into(),
            enabled: true,
            ..Default::default()
        })
        .collect();
    Some(vec![SetSection {
        title: "".into(),
        footer: "".into(),
        rows: ModelRc::new(VecModel::from(rows)),
    }])
}

/// The preview's tiles: the icons the search leaves.
pub fn tiles(set: Option<&IconSet>, query: &str) -> Vec<IconTile> {
    set.map(|s| {
        icon_sets::search(&s.icons, query)
            .into_iter()
            .map(|i| IconTile {
                url: i.url.as_str().into(),
                label: if i.name.is_empty() {
                    "本地图片".into()
                } else {
                    i.label().into()
                },
                selected: false,
            })
            .collect()
    })
    .unwrap_or_default()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn new_id() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros().to_string())
        .unwrap_or_default()
}

/// `_download`: the gallery behind the url, parsed.
fn download(url: &str) -> Result<(String, Vec<IconSetIcon>), String> {
    let d = crate::profile::download(url, true)?;
    icon_sets::parse(&String::from_utf8_lossy(&d.bytes))
}

impl App {
    pub(crate) fn bind_net_images(self: &Arc<Self>, ui: &crate::AppWindow) {
        let images = ui.global::<NetImages>();
        let a = self.clone();
        images.on_has(move |url, _| {
            let found = net_images::cache().lock().unwrap().lookup(&url).is_some();
            a.schedule_net_images();
            found
        });
        images.on_image(|url, _| {
            net_images::cache()
                .lock()
                .unwrap()
                .lookup(&url)
                .map(slint::Image::from_rgba8)
                .unwrap_or_default()
        });
        let a = self.clone();
        ui.on_iconset_search(move |q| {
            preview().lock().unwrap().1 = q.to_string();
            a.publish_icon_preview();
        });
    }

    fn schedule_net_images(self: &Arc<Self>) {
        if !net_images::cache().lock().unwrap().has_pending()
            || FETCHING.swap(true, Ordering::SeqCst)
        {
            return;
        }
        let a = self.clone();
        slint::Timer::single_shot(Duration::from_millis(80), move || {
            a.spawn(|app| app.fetch_net_images());
        });
    }

    /// Four downloads at a time; the page redraws after each round.
    fn fetch_net_images(self: &Arc<Self>) {
        let dir = self.home.join("cache").join("icons");
        loop {
            let urls = net_images::cache().lock().unwrap().take_pending(4);
            if urls.is_empty() {
                break;
            }
            let handles: Vec<_> = urls
                .into_iter()
                .map(|url| {
                    let dir = dir.clone();
                    std::thread::spawn(move || {
                        let image = net_images::load(&dir, &url);
                        net_images::cache().lock().unwrap().put(url, image);
                    })
                })
                .collect();
            for h in handles {
                let _ = h.join();
            }
            self.ui(|ui| {
                let images = ui.global::<NetImages>();
                images.set_rev(images.get_rev() + 1);
            });
        }
        FETCHING.store(false, Ordering::SeqCst);
        if net_images::cache().lock().unwrap().has_pending() {
            self.schedule_net_images();
        }
    }

    fn icon_sets(&self) -> Vec<IconSet> {
        self.store.lock().unwrap().state.settings.icon_sets.clone()
    }

    fn update_icon_sets(self: &Arc<Self>, f: impl FnOnce(&mut Vec<IconSet>)) {
        {
            let mut store = self.store.lock().unwrap();
            f(&mut store.state.settings.icon_sets);
            store.save();
        }
        self.publish_settings();
        self.publish_icon_preview();
    }

    fn publish_icon_preview(self: &Arc<Self>) {
        let (id, query) = preview().lock().unwrap().clone();
        if id.is_empty() {
            return;
        }
        let sets = self.icon_sets();
        let set = sets.iter().find(|s| s.id == id);
        let title: SharedString = set.map(|s| s.name.as_str()).unwrap_or("图标集").into();
        let tiles = tiles(set, &query);
        let searching = !query.trim().is_empty();
        self.ui(move |ui| {
            ui.set_iconset_title(title);
            ui.set_iconset_empty(
                if searching {
                    "暂无搜索结果"
                } else {
                    "暂无图标"
                }
                .into(),
            );
            ui.set_iconset_tiles(ModelRc::new(VecModel::from(tiles)));
        });
    }

    fn open_icon_preview(self: &Arc<Self>, id: &str) {
        *preview().lock().unwrap() = (id.to_string(), String::new());
        self.publish_icon_preview();
        self.ui(|ui| {
            ui.set_iconset_query("".into());
            ui.set_iconset_searching(false);
            ui.set_tools_page("icon-set".into());
        });
    }

    fn icon_set_names(&self) -> Vec<SharedString> {
        self.icon_sets()
            .iter()
            .map(|s| s.name.as_str().into())
            .collect()
    }

    pub(crate) fn icon_sets_clicked(self: &Arc<Self>, page: &str, id: &str) -> bool {
        if page != "icon-sets" {
            return false;
        }
        if id == "action" {
            let item = |id: &str, glyph: &str, label: &str| ActionItem {
                id: format!("is-add:{id}").into(),
                glyph: glyph.into(),
                label: label.into(),
                enabled: true,
                ..Default::default()
            };
            let items = vec![
                item("url", "cloud", "通过URL导入"),
                item("file", "import-file", "通过文件导入"),
            ];
            self.ui(move |ui| {
                ui.set_choice_title("".into());
                ui.set_choice_items(ModelRc::new(VecModel::from(items)));
                ui.set_choice_open(true);
            });
        } else if id == "action2" {
            let names = self.icon_set_names();
            self.ui(move |ui| {
                ui.set_set_sort_titles(ModelRc::new(VecModel::from(names)));
                ui.set_set_sorting(true);
            });
        } else if let Some(rest) = id.strip_prefix("sort-move:") {
            let mut it = rest.split(':').filter_map(|n| n.parse::<usize>().ok());
            if let (Some(from), Some(to)) = (it.next(), it.next()) {
                self.update_icon_sets(|s| crate::reorder::move_item(s, from, to));
                let names = self.icon_set_names();
                self.ui(move |ui| ui.set_set_sort_titles(ModelRc::new(VecModel::from(names))));
            }
        } else if let Some(sid) = id.strip_prefix("long:is:") {
            let sets = self.icon_sets();
            if let Some(i) = sets.iter().position(|s| s.id == sid) {
                self.icon_set_menu(i, &sets[i]);
            }
        } else if let Some(sid) = id.strip_prefix("is:") {
            self.open_icon_preview(sid);
        }
        true
    }

    /// `_buildMenuItems`, anchored at the row.
    fn icon_set_menu(self: &Arc<Self>, index: usize, s: &IconSet) {
        let item = |id: &str, glyph: &str, label: &str, danger: bool| ActionItem {
            id: id.into(),
            glyph: glyph.into(),
            label: label.into(),
            danger,
            enabled: true,
            ..Default::default()
        };
        let mut items = Vec::new();
        if s.is_remote() {
            items.push(item("sync", "sync", "同步", false));
        }
        items.push(item("options", "settings", "选项", false));
        items.push(item("delete", "delete", "删除", true));
        self.ui(move |ui| {
            let anchor = ui.global::<Anchor>();
            ui.set_ctx_names(ModelRc::default());
            ui.set_ctx_items(ModelRc::new(VecModel::from(items)));
            ui.set_ctx_a(index.to_string().into());
            ui.set_ctx_x(anchor.get_x());
            ui.set_ctx_y(anchor.get_y());
            ui.set_ctx_w(anchor.get_w());
            ui.set_ctx_h(anchor.get_h());
            ui.set_ctx_kind("is".into());
        });
    }

    pub(crate) fn icon_set_menu_picked(self: &Arc<Self>, index: usize, action: &str) {
        let Some(s) = self.icon_sets().get(index).cloned() else {
            return;
        };
        match action {
            "sync" => self.spawn(move |app| app.sync_icon_set(s)),
            "options" if s.is_remote() => self.open_input_with(
                &format!("is-opt:{}", s.id),
                "选项",
                "名称",
                &s.name,
                "URL",
                &s.url,
                false,
            ),
            "options" => self.open_input_with(
                &format!("is-opt:{}", s.id),
                "选项",
                "名称",
                &s.name,
                "",
                "",
                false,
            ),
            "delete" => self.confirm(
                "提示",
                format!("确定删除当前{}吗？", s.name),
                "is-delete",
                s.id,
            ),
            _ => {}
        }
    }

    pub(crate) fn icon_sets_check(&self, key: &str, v1: &str, v2: &str) -> Option<String> {
        let issue = match key {
            "is-url" => crate::rule_sets::url_issue(v2),
            k => {
                let id = k.strip_prefix("is-opt:")?;
                let remote = self.icon_sets().iter().any(|s| s.id == id && s.is_remote());
                if remote {
                    crate::rule_sets::url_issue(v2)
                } else if v1.trim().is_empty() {
                    Some("名称不能为空".into())
                } else {
                    None
                }
            }
        };
        Some(issue.unwrap_or_default())
    }

    pub(crate) fn icon_sets_input(self: &Arc<Self>, key: &str, v1: &str, v2: &str) -> bool {
        match key {
            "is-url" => {
                let (url, name) = (v2.trim().to_string(), v1.trim().to_string());
                self.spawn(move |app| app.import_icon_set_url(url, name));
            }
            k if k.starts_with("is-opt:") => {
                let Some(old) = self
                    .icon_sets()
                    .into_iter()
                    .find(|s| s.id == k["is-opt:".len()..])
                else {
                    return true;
                };
                let mut next = old.clone();
                if old.is_remote() {
                    if !v1.trim().is_empty() {
                        next.name = v1.trim().to_string();
                    }
                    next.url = v2.trim().to_string();
                } else {
                    next.name = v1.trim().to_string();
                }
                if next.url == old.url {
                    self.put_icon_set(next);
                } else {
                    self.spawn(move |app| app.sync_icon_set(next));
                }
            }
            _ => return false,
        }
        true
    }

    pub(crate) fn icon_sets_picked(self: &Arc<Self>, key: &str, value: &str) -> bool {
        if key != "is-add" {
            return false;
        }
        match value {
            "url" => self.open_input_with("is-url", "通过URL导入", "名称", "", "URL", "", false),
            "file" => self.spawn(|app| app.import_icon_set_file()),
            _ => {}
        }
        true
    }

    fn put_icon_set(self: &Arc<Self>, s: IconSet) {
        self.update_icon_sets(move |sets| match sets.iter_mut().find(|x| x.id == s.id) {
            Some(x) => *x = s,
            None => sets.push(s),
        });
    }

    /// `importUrl`: a url already kept syncs that set instead of adding another.
    fn import_icon_set_url(self: &Arc<Self>, url: String, name: String) {
        if let Some(mut existing) = self.icon_sets().into_iter().find(|s| s.url == url) {
            if !name.is_empty() {
                existing.name = name;
            }
            return self.sync_icon_set(existing);
        }
        match download(&url) {
            Ok((parsed, icons)) => self.put_icon_set(IconSet {
                id: new_id(),
                name: icon_sets::name_of(&name, &parsed, &icon_sets::url_fallback(&url)),
                url,
                icons,
                updated: now_secs(),
            }),
            Err(e) => self.toast(e),
        }
    }

    /// `sync`: a set whose url changed or that was deleted meanwhile is left as it now is.
    fn sync_icon_set(self: &Arc<Self>, s: IconSet) {
        let before = self.icon_sets().into_iter().find(|x| x.id == s.id);
        let (_, icons) = match download(&s.url) {
            Ok(r) => r,
            Err(e) => return self.toast(e),
        };
        let latest = self.icon_sets().into_iter().find(|x| x.id == s.id);
        let base = match (before, latest) {
            (Some(b), Some(l)) if l.url == b.url => IconSet {
                name: if s.name == b.name { l.name } else { s.name },
                url: s.url,
                ..l
            },
            (Some(_), _) => return,
            (None, _) => s,
        };
        self.put_icon_set(IconSet {
            icons,
            updated: now_secs(),
            ..base
        });
    }

    fn import_icon_set_file(self: &Arc<Self>) {
        let dir = self.home.join("cache");
        let _ = std::fs::create_dir_all(&dir);
        let tmp = dir.join("icon-set.part");
        let picked = self.bridge().and_then(|b| {
            b.shell(
                "file.open",
                json!({"dest": tmp.to_string_lossy()}),
                Duration::from_secs(3600),
            )
        });
        let name = match picked {
            Ok(r) if r.get("cancelled").and_then(Value::as_bool) == Some(true) => return,
            Ok(r) => r
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            Err(e) => return self.toast(e),
        };
        let bytes = std::fs::read(&tmp).unwrap_or_default();
        let _ = std::fs::remove_file(&tmp);
        match icon_sets::parse(&String::from_utf8_lossy(&bytes)) {
            Ok((parsed, icons)) => self.put_icon_set(IconSet {
                id: new_id(),
                name: icon_sets::name_of("", &parsed, &crate::rule_sets::file_stem(&name)),
                url: String::new(),
                icons,
                updated: now_secs(),
            }),
            Err(e) => self.toast(e),
        }
    }

    pub(crate) fn delete_icon_set(self: &Arc<Self>, id: &str) {
        let id = id.to_string();
        self.update_icon_sets(move |sets| sets.retain(|x| x.id != id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set() -> IconSet {
        IconSet {
            id: "1".into(),
            name: "Qure".into(),
            url: "https://h.example/qure.json".into(),
            icons: vec![
                IconSetIcon {
                    name: "Hong_Kong.png".into(),
                    url: "https://h.example/hk.png".into(),
                },
                IconSetIcon {
                    name: "".into(),
                    url: "https://h.example/x.png".into(),
                },
            ],
            updated: 0,
        }
    }

    #[test]
    fn rows_carry_cover_count_and_origin() {
        let s = sections(&[set()], "icon-sets").unwrap();
        assert_eq!(s.len(), 1);
        assert!(sections(&[], "icon-sets").unwrap().is_empty());
        assert!(sections(&[set()], "rule-sets").is_none());
        let meta = page_meta(&[set()], "icon-sets").unwrap();
        assert_eq!(meta.action2, "");
        assert_eq!(meta.empty, "暂无图标集");
    }

    #[test]
    fn tiles_follow_the_search_and_name_local_images() {
        let s = set();
        let all = tiles(Some(&s), "");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].label, "Hong Kong");
        assert_eq!(all[1].label, "本地图片");
        assert_eq!(tiles(Some(&s), "kong").len(), 1);
        assert!(tiles(None, "").is_empty());
    }
}
