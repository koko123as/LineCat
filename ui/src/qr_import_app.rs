//! The add menu's 二维码 and, on a TV, 从手机发送 (views/profiles/add.dart).

use crate::link_receiver::LinkReceiver;
use crate::qr_decode;
use crate::qr_import::profile_url_from_qr_codes;
use crate::App;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

static RECEIVER: Mutex<Option<LinkReceiver>> = Mutex::new(None);

const CLOSE: Duration = Duration::from_secs(10);

/// The dark scheme's colours the scan page draws with (`watchAppTheme(Brightness.dark)`).
fn scan_colors(p: &crate::theme::Palette) -> Value {
    let hex = |c: u32| format!("#{:06x}", c & 0xff_ffff);
    json!({
        "primary": hex(p.primary),
        "onPrimary": hex(p.on_primary),
        "surface": hex(p.page),
        "onSurface": hex(p.on_surface),
        "muted": hex(p.on_surface_variant),
        "error": hex(p.error),
    })
}

const SEND_TIP: &str = "用手机相机扫描二维码，在打开的页面里粘贴链接。两台设备需要连在同一个网络。";

/// `data` as a black on white QR picture, 8 px a module with a two module quiet zone.
pub(crate) fn qr_picture(data: &str) -> Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>> {
    let code = qrcode::QrCode::new(data.as_bytes()).ok()?;
    let modules = code.width();
    let quiet = 2;
    let scale = 8;
    let side = (modules + quiet * 2) * scale;
    let mut buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(side as u32, side as u32);
    let colors = code.to_colors();
    for (i, px) in buffer.make_mut_slice().iter_mut().enumerate() {
        let (x, y) = (i % side / scale, i / side / scale);
        let dark = x >= quiet
            && y >= quiet
            && x < modules + quiet
            && y < modules + quiet
            && colors[(y - quiet) * modules + (x - quiet)] == qrcode::Color::Dark;
        let v = if dark { 0 } else { 255 };
        *px = slint::Rgba8Pixel {
            r: v,
            g: v,
            b: v,
            a: 255,
        };
    }
    Some(buffer)
}

impl App {
    /// `_toScan`: the scan page's codes (an invalid one keeps it open with its hint, as
    /// ScanPage does), or with no camera a picked picture read here; then `addProfileFromLink`.
    pub(crate) fn add_from_qr(self: &Arc<Self>) {
        self.spawn(|app| {
            let tmp = app.temp_for("qr-luma");
            let colors = {
                let settings = app.store.lock().unwrap().state.settings.clone();
                scan_colors(&crate::theme_app::dark_palette(&settings))
            };
            let mut invalid = Value::Null;
            let url = loop {
                let captured = app.bridge().and_then(|b| {
                    b.shell(
                        "qr.capture",
                        json!({
                            "dest": tmp.to_string_lossy(),
                            "source": "camera",
                            "invalid": invalid,
                            "colors": colors,
                        }),
                        Duration::from_secs(3600),
                    )
                });
                let r = match captured {
                    Ok(r) if r.get("cancelled").and_then(Value::as_bool) == Some(true) => return,
                    Ok(r) => r,
                    Err(e) => return app.toast(e),
                };
                if let Some(codes) = r.get("codes").and_then(Value::as_array) {
                    let codes = codes.iter().filter_map(Value::as_str);
                    match profile_url_from_qr_codes(codes) {
                        Some(url) => {
                            let _ = app
                                .bridge()
                                .and_then(|b| b.shell("qr.close", json!({}), CLOSE));
                            break url;
                        }
                        None => {
                            let album = r.get("album").and_then(Value::as_bool) == Some(true);
                            invalid = json!(if album { "album" } else { "camera" });
                            continue;
                        }
                    }
                }
                let width = r.get("width").and_then(Value::as_u64).unwrap_or(0) as usize;
                let height = r.get("height").and_then(Value::as_u64).unwrap_or(0) as usize;
                let luma = std::fs::read(&tmp).unwrap_or_default();
                let _ = std::fs::remove_file(&tmp);
                let text = qr_decode::decode(&luma, width, height);
                match profile_url_from_qr_codes(text.as_deref()) {
                    Some(url) => break url,
                    None => return app.alert("二维码", "请上传有效的二维码"),
                }
            };
            app.ui(|ui| ui.set_adding(true));
            if let Err(e) = app.add_from_link(String::new(), url, String::new()) {
                app.alert("添加配置", e);
            }
            app.ui(|ui| ui.set_adding(false));
        });
    }

    /// `_toSendFromPhone`: the local network permission, then the receiver's dialog, then
    /// the link the phone sent once it is confirmed.
    pub(crate) fn send_from_phone(self: &Arc<Self>) {
        self.spawn(|app| {
            let granted = app
                .bridge()
                .and_then(|b| b.shell("lan.permission", json!({}), Duration::from_secs(600)))
                .map(|r| r.get("granted").and_then(Value::as_bool).unwrap_or(false));
            match granted {
                Ok(true) => {}
                Ok(false) => {
                    return app.confirm_labeled(
                        "提示",
                        "从手机接收链接需要「附近的设备」权限，SeasonCat 才能访问局域网。请在应用设置中开启。"
                            .into(),
                        "app-settings",
                        String::new(),
                        ("设置", "", false),
                    )
                }
                Err(e) => return app.toast(e),
            }
            let weak = Arc::downgrade(&app);
            let started = LinkReceiver::start(None, move |link| {
                if let Some(app) = weak.upgrade() {
                    app.link_received(link);
                }
            });
            let receiver = started.ok().flatten();
            let address = receiver.as_ref().map(|r| r.address.clone());
            *RECEIVER.lock().unwrap() = receiver;
            let picture = address.as_deref().and_then(qr_picture);
            let text = match address {
                Some(a) => format!("{a}\n\n{SEND_TIP}"),
                None => "这台设备没有连接局域网".to_string(),
            };
            app.ui(move |ui| {
                let image = picture.map(slint::Image::from_rgba8).unwrap_or_default();
                ui.set_qr_title("从手机发送".into());
                ui.set_qr_image(image);
                ui.set_qr_text(text.into());
                ui.set_qr_kind("send".into());
                ui.set_qr_open(true);
            });
        });
    }

    fn link_received(self: &Arc<Self>, link: String) {
        RECEIVER.lock().unwrap().take();
        self.ui(|ui| {
            ui.set_qr_open(false);
            ui.set_qr_kind("".into());
        });
        self.confirm_labeled(
            "导入手机发来的链接？",
            link.clone(),
            "send-import",
            link,
            ("导入", "", false),
        );
    }

    /// The send dialog went away without a link: the page stops being served.
    pub(crate) fn qr_closed(self: &Arc<Self>) {
        RECEIVER.lock().unwrap().take();
    }

    pub(crate) fn import_sent_link(self: &Arc<Self>, link: String) {
        self.spawn(move |app| {
            app.ui(|ui| ui.set_adding(true));
            if let Err(e) = app.add_from_link(String::new(), link, String::new()) {
                app.alert("添加配置", e);
            }
            app.ui(|ui| ui.set_adding(false));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scan_page_takes_the_dark_scheme() {
        let p = crate::colors::palette(0x007aff, crate::colors::VIVID, true);
        let c = scan_colors(&p);
        assert_eq!(c["primary"], "#2f83ff");
        assert_eq!(c["error"], "#ffb4ab");
        assert_eq!(c["surface"].as_str().map(str::len), Some(7));
    }
}
