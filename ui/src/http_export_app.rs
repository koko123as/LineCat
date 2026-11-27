//! A captured request's menu on `App`'s side: what each item reads from the Core and where its
//! result goes. FlClash: lib/features/connection/mitm_exchange_menu.dart.

use crate::http_body::{self, Raw};
use crate::http_export::{self, Exchange, MenuTarget};
use crate::{App, CALL};
use serde_json::{json, Value};
use slint::{ModelRc, VecModel};
use std::sync::Arc;
use std::time::Duration;

const APP_NAME: &str = "SeasonCat";

impl App {
    fn local_modules(&self) -> Vec<(String, String)> {
        self.mitm_settings()
            .mitm_modules
            .into_iter()
            .filter(|m| !m.is_remote())
            .map(|m| (m.id, m.name))
            .collect()
    }

    pub(crate) fn open_http_menu(self: &Arc<Self>, target: MenuTarget) {
        let items = http_export::menu_items(&target, &self.local_modules(), None);
        self.req.lock().unwrap().http_menu = Some((target, None));
        self.show_actions(String::new(), items);
    }

    /// The menu's item `name` with its request id (and module) in `arg`; false for an item
    /// that is not the menu's.
    pub(crate) fn http_menu_picked(self: &Arc<Self>, name: &str, arg: &str) -> bool {
        let (id, module) = arg.split_once(':').unwrap_or((arg, ""));
        let id: i64 = id.parse().unwrap_or(0);
        match name {
            http_export::REJECT | http_export::MAP_LOCAL_ITEM => self.unfold_targets(name),
            "http-copy-url" => {
                let url = self
                    .req
                    .lock()
                    .unwrap()
                    .http_menu
                    .as_ref()
                    .filter(|(t, _)| t.id == id)
                    .map(|(t, _)| t.url.clone());
                if let Some(url) = url {
                    self.copy(url);
                }
            }
            "http-curl" => self.export(name, id, "复制为 cURL", |app, e| {
                let body = app.http_body_of(&e, false)?;
                app.copy(http_export::curl(&e, body.as_ref()));
                Ok(())
            }),
            "http-copy-request" => self.export(name, id, "复制请求", |app, e| {
                let body = app.http_body_of(&e, false)?;
                app.copy(http_export::raw_request(&e, body.as_ref()));
                Ok(())
            }),
            "http-copy-response" => self.export(name, id, "复制响应", |app, e| {
                let body = app.http_body_of(&e, true)?;
                app.copy(http_export::raw_response(&e, body.as_ref()));
                Ok(())
            }),
            "http-copy-body" => self.export(name, id, "复制响应体", |app, e| {
                if let Some(body) = app.http_body_of(&e, true)? {
                    app.copy(http_body::copy_text(&body));
                }
                Ok(())
            }),
            "http-har" => self.export(name, id, "导出 HAR", |app, e| app.export_har(&e)),
            "http-reject-to" => {
                let module = module.to_string();
                self.with_exchange(id, move |app, e| {
                    let line = http_export::reject_line(e.asked_url());
                    app.add_module_rule(&module, http_export::URL_REWRITE, line, None);
                });
            }
            "http-map-local-to" => {
                let module = module.to_string();
                self.with_exchange(id, move |app, e| app.add_map_local(&e, &module));
            }
            _ => return false,
        }
        true
    }

    fn unfold_targets(self: &Arc<Self>, name: &str) {
        let modules = self.local_modules();
        let items = {
            let mut req = self.req.lock().unwrap();
            let Some((target, open)) = req.http_menu.as_mut() else {
                return;
            };
            *open = if open.as_deref() == Some(name) {
                None
            } else {
                Some(name.to_string())
            };
            http_export::menu_items(target, &modules, open.as_deref())
        };
        self.ui(move |ui| ui.set_req_actions(ModelRc::new(VecModel::from(items))));
    }

    /// `withExchange`: the whole request, read again, since a list knows only its summary.
    fn with_exchange(
        self: &Arc<Self>,
        id: i64,
        f: impl FnOnce(&Arc<App>, Exchange) + Send + 'static,
    ) {
        self.spawn(move |app| {
            let exchange = app
                .bridge()
                .and_then(|b| b.call("mitm.exchange", json!({"id": id}), CALL))
                .ok()
                .and_then(|v| Exchange::parse(&v));
            match exchange {
                Some(e) => f(&app, e),
                None => app.toast(http_export::CLEARED),
            }
        });
    }

    /// An action that hands the request out, once the user was told what it can carry
    /// (`_warned`); its failure shows under `title`.
    fn export(
        self: &Arc<Self>,
        name: &str,
        id: i64,
        title: &'static str,
        f: impl FnOnce(&Arc<App>, Exchange) -> Result<(), String> + Send + 'static,
    ) {
        let action = format!("{name}:{id}");
        self.with_exchange(id, move |app, e| {
            if !app.req.lock().unwrap().export_warned {
                app.confirm_labeled(
                    http_export::PRIVATE_TITLE,
                    http_export::PRIVATE.into(),
                    "http-warned",
                    action,
                    (title, "", false),
                );
                return;
            }
            if let Err(err) = f(app, e) {
                app.alert(title, err);
            }
        });
    }

    /// The user agreed to hand out a request: not asked again this run.
    pub(crate) fn http_warned(self: &Arc<Self>, action: &str) {
        self.req.lock().unwrap().export_warned = true;
        let (name, arg) = action.split_once(':').unwrap_or((action, ""));
        self.http_menu_picked(name, arg);
    }

    /// `_body`: None for a side with nothing kept, or still in flight.
    fn http_body_of(&self, e: &Exchange, response: bool) -> Result<Option<Raw>, String> {
        let size = if response {
            e.response_size
        } else {
            e.request_size
        };
        if size == 0 || e.pending {
            return Ok(None);
        }
        let v =
            self.bridge()?
                .call("mitm.body", json!({"id": e.id, "response": response}), CALL)?;
        Ok(v.is_object().then(|| Raw::parse(&v)))
    }

    fn export_har(self: &Arc<Self>, e: &Exchange) -> Result<(), String> {
        let request = self.http_body_of(e, false)?;
        let response = self.http_body_of(e, true)?;
        let version = self.app_version.lock().unwrap().clone();
        let text = http_export::har(e, request.as_ref(), response.as_ref(), APP_NAME, &version);
        let path = self.store.lock().unwrap().dir.join("request.export.har");
        std::fs::write(&path, text).map_err(|err| err.to_string())?;
        let saved = self.bridge().and_then(|b| {
            b.shell(
                "file.save",
                json!({"src": path.to_string_lossy(), "name": http_export::har_file_name(e, APP_NAME)}),
                Duration::from_secs(3600),
            )
        });
        let _ = std::fs::remove_file(&path);
        if saved?.get("cancelled").and_then(Value::as_bool) != Some(true) {
            self.toast("导出成功");
        }
        Ok(())
    }

    /// `_addMapLocal`: the response kept as a file beside the rule, when it was kept whole and
    /// decoded.
    fn add_map_local(self: &Arc<Self>, e: &Exchange, module: &str) {
        let body = match self.http_body_of(e, true) {
            Ok(body) => body,
            Err(err) => return self.alert("添加本地映射规则", err),
        };
        let bytes = body
            .filter(|b| !b.truncated && b.encoding.is_empty())
            .map(|b| b.bytes())
            .unwrap_or_default();
        let taken: Vec<String> = self
            .mitm_settings()
            .mitm_modules
            .iter()
            .find(|m| m.id == module)
            .map(|m| m.attachments().into_keys().collect())
            .unwrap_or_default();
        let file = (!bytes.is_empty()).then(|| http_export::attachment_name(&e.url, &taken));
        let content_type = http_body::header_value(&e.response_header, "Content-Type");
        let line = http_export::map_local_line(&e.url, file.as_deref(), e.status, &content_type);
        self.add_module_rule(
            module,
            http_export::MAP_LOCAL,
            line,
            file.map(|f| (f, bytes)),
        );
    }
}
