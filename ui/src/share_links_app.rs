//! Share links on `App`'s side: the list's ⊕ menu (添加 / 快速编辑) with the quick edit of every
//! proxy as links in the tools editor (`_handleQuickEdit`), and the form's 快速编辑 that fills the
//! form from one link (`_ShareLinkDialog`, `_fillFromLink`).

use crate::custom_proxies::CustomProxy;
use crate::custom_proxies_app::LIST;
use crate::share_links::{self as sl, ShareLinkEdit};
use crate::{ActionItem, Anchor, App, CALL};
use serde_json::{json, Map, Value};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct State {
    /// The list as the quick edit opened it, with each proxy's link.
    list: Option<(Vec<CustomProxy>, Vec<String>)>,
    /// What a confirmed quick edit sets the list to.
    pending: Option<Vec<CustomProxy>>,
    link: Option<String>,
    /// Counts the dialog's openings, so a read that ends after it closed is dropped.
    opened: u64,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(STATE.lock().unwrap().get_or_insert_with(Default::default))
}

const LINK_KEY: &str = "cp-link";

impl App {
    pub(crate) fn group_names(&self) -> BTreeSet<String> {
        self.store
            .lock()
            .unwrap()
            .state
            .proxy_groups
            .iter()
            .map(|g| g.name().to_string())
            .collect()
    }

    fn encode_links(&self, definitions: Vec<Value>) -> Result<Vec<String>, String> {
        if definitions.is_empty() {
            return Ok(Vec::new());
        }
        let n = definitions.len();
        let r = self.bridge()?.call(
            "convert.encodeLinks",
            json!({ "proxies": definitions }),
            CALL,
        )?;
        Ok(sl::parse_encoded(&r, n))
    }

    fn decode_links(&self, lines: Vec<String>) -> Result<Vec<Vec<Value>>, String> {
        if lines.is_empty() {
            return Ok(Vec::new());
        }
        let r = self
            .bridge()?
            .call("convert.decodeLinks", json!({ "lines": lines }), CALL)?;
        Ok(sl::parse_decoded(&r))
    }

    /// `ListEditorAddAction` with a quick edit: 添加 and 快速编辑 anchored at the bar's ⊕.
    pub(crate) fn custom_proxies_add_menu(self: &Arc<Self>) {
        let item = |id: &str, glyph: &str, label: &str| ActionItem {
            id: id.into(),
            glyph: glyph.into(),
            label: label.into(),
            enabled: true,
            ..Default::default()
        };
        let items = vec![
            item("add", "plus", "添加"),
            item("quick-edit", "compose", "快速编辑"),
        ];
        self.ui(move |ui| {
            let anchor = ui.global::<Anchor>();
            ui.set_ctx_names(ModelRc::default());
            ui.set_ctx_items(ModelRc::new(VecModel::from(items)));
            ui.set_ctx_a("0".into());
            ui.set_ctx_x(anchor.get_x());
            ui.set_ctx_y(anchor.get_y());
            ui.set_ctx_w(anchor.get_w());
            ui.set_ctx_h(anchor.get_h());
            ui.set_ctx_kind("cp-add".into());
        });
    }

    pub(crate) fn custom_proxies_add_picked(self: &Arc<Self>, action: &str) {
        match action {
            "add" => self.open_custom_proxy(None),
            "quick-edit" => self.spawn(|app| app.open_links_edit()),
            _ => {}
        }
    }

    fn open_links_edit(self: &Arc<Self>) {
        crate::rule_sets_app::release_editor();
        with(|s| s.list = None);
        self.ui(move |ui| {
            ui.set_tools_editor_title("代理".into());
            ui.set_tools_editor_text(SharedString::new());
            ui.set_tools_editor_guard(false);
            ui.set_tools_editor_parent(LIST.into());
            ui.set_tools_editor_readonly(false);
            ui.set_tools_editor_title_editable(false);
            ui.set_tools_editor_error(SharedString::new());
            ui.set_tools_editor_loading(true);
            ui.set_tools_page("editor".into());
        });
        let load = crate::items_app::editor_load();
        let previous = self.custom_proxies();
        match self.encode_links(previous.iter().map(|p| p.definition.clone()).collect()) {
            Ok(links) => {
                let text = ShareLinkEdit::text_of(&links);
                self.tools_editor_loaded(load, Ok(text), move || {
                    with(|s| s.list = Some((previous, links)))
                });
            }
            Err(e) => self.tools_editor_loaded(load, Err(e), || {}),
        }
    }

    /// The tools editor's save while the links' quick edit is open; false otherwise.
    pub(crate) fn share_links_editor_save(self: &Arc<Self>, text: &str) -> bool {
        let Some((previous, links)) = with(|s| s.list.clone()) else {
            return false;
        };
        let text = text.to_string();
        self.spawn(
            move |app| match app.read_links_edit(&text, &previous, &links) {
                Ok(next) => match sl::replaced_message(&previous, &next) {
                    Some(message) => {
                        with(|s| s.pending = Some(next));
                        app.confirm("提示", message, "cp-links-replace", String::new());
                    }
                    None => app.set_linked_proxies(next),
                },
                Err(e) => app.alert("无法保存", e),
            },
        );
        true
    }

    fn read_links_edit(
        &self,
        text: &str,
        previous: &[CustomProxy],
        links: &[String],
    ) -> Result<Vec<CustomProxy>, String> {
        let edit = ShareLinkEdit::read(text, previous, links);
        let pending = edit.pending();
        let results = self.decode_links(pending.iter().map(|l| l.link.clone()).collect())?;
        let decoded = sl::decoded_lines(&pending, &results)?;
        let entries = sl::entries(&decoded);
        let definitions: Vec<Value> = entries.iter().map(|(_, d)| d.clone()).collect();
        if !definitions.is_empty() {
            let verdicts = self.validate_proxies(&definitions)?;
            if let Some((i, m)) = verdicts
                .into_iter()
                .enumerate()
                .find_map(|(i, v)| v.map(|m| (i, m)))
            {
                return Err(sl::line_issue(entries[i].0, &m));
            }
        }
        let dropped = edit.dropped_links();
        let read_back = self.decode_links(dropped.iter().map(|(_, l)| l.clone()).collect())?;
        let linked: HashMap<String, Map<String, Value>> = dropped
            .iter()
            .zip(read_back)
            .filter_map(|((id, _), r)| match r.as_slice() {
                [one] => one.as_object().map(|m| (id.clone(), m.clone())),
                _ => None,
            })
            .collect();
        let mut next: u128 = crate::custom_proxies_app::new_id()
            .parse()
            .unwrap_or_default();
        Ok(edit.apply(&decoded, &self.group_names(), &linked, || {
            next += 1;
            next.to_string()
        }))
    }

    fn set_linked_proxies(self: &Arc<Self>, proxies: Vec<CustomProxy>) {
        with(|s| {
            s.list = None;
            s.pending = None;
        });
        self.update_custom_proxies(move |s| *s = proxies);
        self.ui(|ui| ui.set_tools_page(LIST.into()));
    }

    /// `cp-links-replace` answered.
    pub(crate) fn links_replace_answer(self: &Arc<Self>, yes: bool) {
        match with(|s| s.pending.take()) {
            Some(next) if yes => self.set_linked_proxies(next),
            _ => {}
        }
    }

    /// The tools page moved off the editor: the quick edit is over.
    pub(crate) fn share_links_page_shown(&self, page: &str) {
        if page != "editor" {
            with(|s| s.list = None);
        }
    }

    /// The form's 快速编辑: the proxy as its link in the dialog (empty when it has none).
    pub(crate) fn open_link_dialog(self: &Arc<Self>) {
        let Some(d) = self.draft() else {
            return;
        };
        self.spawn(move |app| {
            let link = app
                .encode_links(vec![d.proxy.definition.clone()])
                .ok()
                .and_then(|l| l.into_iter().next())
                .unwrap_or_default();
            with(|s| {
                s.link = Some(link.clone());
                s.opened += 1;
            });
            app.open_input(LINK_KEY, "快速编辑", "", &link);
            app.ui(|ui| {
                ui.set_input_hint("vless://…".into());
                ui.set_input_lines(3);
                ui.set_input_holds(true);
            });
        });
    }

    /// `_submit`: an unchanged link closes; another is read with 确定 disabled, then fills or fails.
    pub(crate) fn share_links_input(self: &Arc<Self>, key: &str, v1: &str) -> bool {
        if key != LINK_KEY {
            return false;
        }
        let text = v1.trim().to_string();
        let Some(link) = with(|s| s.link.clone()) else {
            return true;
        };
        if text == link {
            with(|s| s.link = None);
            self.ui(|ui| ui.set_input_open(false));
            return true;
        }
        let opened = with(|s| s.opened);
        self.spawn(move |app| {
            let Some(d) = app.draft() else {
                return;
            };
            let mut lines = vec![text.clone()];
            if !link.is_empty() {
                lines.push(link.clone());
            }
            let read = app
                .decode_links(lines)
                .and_then(|r| sl::single_definition(&r, &d.proxy.map()));
            let named = read.map(|definition| {
                sl::named_from_link(
                    definition,
                    &d.proxy,
                    &app.custom_proxies(),
                    &app.group_names(),
                )
            });
            let a = app.clone();
            app.ui(move |ui| {
                let current = with(|s| s.opened == opened && s.link.is_some());
                if !current || !ui.get_input_open() || ui.get_input_key() != LINK_KEY {
                    return;
                }
                match named {
                    Ok(named) => {
                        with(|s| s.link = None);
                        ui.set_input_open(false);
                        a.edit_draft(move |d| d.fill(named));
                    }
                    Err(e) => {
                        ui.set_input_fail(e.into());
                        ui.set_input_busy(false);
                    }
                }
            });
        });
        true
    }
}
