//! Renders every page with sample data to PNG files, so the layout can be checked on a
//! desktop without a device: `cargo run --example snapshot -- <out dir>`.
use seasoncat_ui::*;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{Model, ModelRc, Rgb8Pixel, VecModel};
use std::rc::Rc;

struct Headless(Rc<MinimalSoftwareWindow>);

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let (w, h) = (412u32, 892u32);
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
    let ui = AppWindow::new().unwrap();
    window.set_size(slint::PhysicalSize::new(w, h));

    let [light, dark] = theme::SCHEMES[0];
    let scheme = |p: &theme::Palette| {
        let c = |v: u32| slint::Color::from_argb_encoded(0xff00_0000 | v);
        Scheme {
            primary: c(p.primary),
            on_primary: c(p.on_primary),
            primary_container: c(p.primary_container),
            on_primary_container: c(p.on_primary_container),
            page: c(p.page),
            card: c(p.card),
            block: c(p.block),
            inset: c(p.inset),
            on_surface: c(p.on_surface),
            on_surface_variant: c(p.on_surface_variant),
            outline_variant: c(p.outline_variant),
            error: c(p.error),
        }
    };
    ui.global::<Theme>().set_light(scheme(&light));
    ui.global::<Theme>().set_dark(scheme(&dark));
    ui.set_accents(ModelRc::new(VecModel::from(
        theme::ACCENTS
            .iter()
            .map(|c| slint::Color::from_argb_encoded(0xff00_0000 | c))
            .collect::<Vec<_>>(),
    )));
    ui.set_run_state("STARTED".into());
    ui.set_profile_name("我的机场".into());
    ui.set_up_speed("12.3 KB/s".into());
    ui.set_down_speed("1.4 MB/s".into());
    ui.set_up_total("共 3.2 MB".into());
    ui.set_down_total("共 120.5 MB".into());
    ui.set_version("UI 0.1.0 · Concord 0.9.5".into());
    ui.set_profiles(ModelRc::new(VecModel::from(vec![
        ProfileItem {
            id: "1".into(),
            name: "我的机场".into(),
            subtitle: "35.2 GB / 200.0 GB".into(),
            trailing: "3 分钟前".into(),
            expire: "2027-03-01".into(),
            expired: false,
            active: true,
            busy: false,
            is_url: true,
        },
        ProfileItem {
            id: "2".into(),
            name: "备用".into(),
            subtitle: "远程".into(),
            trailing: "2 天前".into(),
            expire: "2026-10-01".into(),
            expired: true,
            active: false,
            busy: true,
            is_url: true,
        },
        ProfileItem {
            id: "3".into(),
            name: "本地配置.yaml".into(),
            subtitle: "本地".into(),
            trailing: "1 个月前".into(),
            expire: "".into(),
            expired: false,
            active: false,
            busy: false,
            is_url: false,
        },
    ])));
    ui.set_sort_items(ui.get_profiles());
    ui.set_edit_is_url(true);
    ui.set_edit_name("我的机场".into());
    ui.set_edit_url("https://sub.example.com/api/v1/client/subscribe?token=abc".into());
    ui.set_edit_auto(true);
    ui.set_edit_interval("1440".into());
    ui.set_edit_file_info("86.4 KB · 3 分钟前".into());
    ui.set_editor_title("我的机场".into());
    ui.set_editor_text("mixed-port: 7890\nmode: rule\nproxies:\n  - name: 香港 01\n    type: ss\n    server: hk.example.com\n    port: 443\n".into());
    ui.set_editor_readonly(true);
    ui.set_groups(ModelRc::new(VecModel::from(vec![
        GroupItem {
            name: "节点选择".into(),
            kind: "Selector".into(),
            now: "香港 01".into(),
            count: 24,
        },
        GroupItem {
            name: "自动选择".into(),
            kind: "URLTest".into(),
            now: "日本 03".into(),
            count: 23,
        },
        GroupItem {
            name: "流媒体".into(),
            kind: "Selector".into(),
            now: "节点选择".into(),
            count: 3,
        },
    ])));
    ui.set_nodes(ModelRc::new(VecModel::from(vec![
        NodeItem {
            name: "香港 01".into(),
            kind: "Shadowsocks".into(),
            delay: 86,
            selected: true,
        },
        NodeItem {
            name: "日本 03".into(),
            kind: "Trojan".into(),
            delay: 1203,
            selected: false,
        },
        NodeItem {
            name: "美国 洛杉矶 高速".into(),
            kind: "VMess".into(),
            delay: -1,
            selected: false,
        },
        NodeItem {
            name: "新加坡".into(),
            kind: "Hysteria2".into(),
            delay: -3,
            selected: false,
        },
    ])));
    let logs = VecModel::<LogItem>::default();
    for (level, text) in [("info", "[TCP] 192.168.1.5:51234 --> www.google.com:443 match RuleSet(proxy) using 节点选择[香港 01]"), ("warning", "dns: lookup timeout for example.invalid"), ("error", "dial 美国 洛杉矶 高速 failed: connection refused")] {
        logs.push(LogItem { level: level.into(), time: "03:41:07".into(), text: text.into() });
    }
    ui.set_logs(ModelRc::new(logs));
    let _ = ui.get_logs().row_count();

    // (file, tab, open group, tools or profiles page, theme mode, overlay)
    let shots: &[(&str, i32, &str, &str, i32, &str)] = &[
        ("dashboard", 0, "", "", 1, ""),
        ("proxies", 1, "节点选择", "", 1, ""),
        ("profiles", 2, "", "", 1, ""),
        ("profiles-add-menu", 2, "", "", 1, "add-menu"),
        ("profiles-link", 2, "", "", 1, "link"),
        ("profiles-menu", 2, "", "", 1, "menu"),
        ("profiles-confirm", 2, "", "", 1, "confirm"),
        ("profiles-edit", 2, "", "edit", 1, ""),
        ("profiles-preview", 2, "", "preview", 1, ""),
        ("profiles-sort", 2, "", "sort", 1, ""),
        ("tools", 3, "", "", 1, ""),
        ("logs", 3, "", "logs", 1, ""),
        ("theme", 3, "", "theme", 1, ""),
        ("dashboard-dark", 0, "", "", 2, ""),
        ("profiles-dark", 2, "", "", 2, ""),
    ];
    ui.show().unwrap();
    for (name, tab, group, page, mode, overlay) in shots {
        ui.set_tab(*tab);
        ui.set_current_group((*group).into());
        if *tab == 2 {
            ui.set_profiles_page((*page).into());
            ui.set_tools_page("".into());
        } else {
            ui.set_tools_page((*page).into());
            ui.set_profiles_page("".into());
        }
        ui.set_add_menu_open(*overlay == "add-menu");
        ui.set_link_open(*overlay == "link");
        ui.set_menu_open(*overlay == "menu");
        ui.set_menu_more(true);
        ui.set_menu_item(ui.get_profiles().row_data(0).unwrap());
        ui.set_confirm_title("提示".into());
        ui.set_confirm_text("确定删除当前配置吗？".into());
        ui.set_confirm_action("delete".into());
        ui.set_confirm_open(*overlay == "confirm");
        ui.global::<Theme>().set_mode(*mode);
        ui.set_theme_mode(*mode);
        slint::platform::update_timers_and_animations();
        let mut buffer = vec![Rgb8Pixel::default(); (w * h) as usize];
        window.request_redraw();
        window.draw_if_needed(|r| {
            r.render(&mut buffer, w as usize);
        });
        let file = std::fs::File::create(format!("{out}/{name}.png")).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let bytes: Vec<u8> = buffer.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
        enc.write_header()
            .unwrap()
            .write_image_data(&bytes)
            .unwrap();
        println!("{out}/{name}.png");
    }
}
