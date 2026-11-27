//! Sheets and pushed pages over a tab, and the order back takes them in.
use seasoncat_ui::*;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Key, Platform, WindowAdapter, WindowEvent};
use std::rc::Rc;

struct Headless(Rc<MinimalSoftwareWindow>);

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

fn back(ui: &AppWindow) {
    let text: slint::SharedString = Key::Back.into();
    let w = ui.window();
    w.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    w.dispatch_event(WindowEvent::KeyReleased { text });
}

#[test]
fn navigation() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
    window.set_size(slint::PhysicalSize::new(412, 892));
    let ui = AppWindow::new().unwrap();
    ui.on_page_meta(|page, _| PageMeta {
        known: page == "scripts" || page == "advanced",
        title: page.clone(),
        parent: if page == "scripts" { "advanced" } else { "" }.into(),
        ..Default::default()
    });
    let feeds = Rc::new(std::cell::RefCell::new(Vec::new()));
    let f = feeds.clone();
    ui.on_req_feed(move |open, segment| f.borrow_mut().push((open, segment)));
    ui.show().unwrap();

    // 前往配置脚本 pushes 脚本 over the extend page; back returns to it, not to 进阶配置.
    ui.set_tab(2);
    ui.set_profiles_page("extend".into());
    ui.invoke_open_tools("scripts".into());
    assert_eq!(ui.get_tab(), 2);
    assert!(ui.get_tools_over());
    assert_eq!(ui.get_tools_page(), "scripts");
    back(&ui);
    assert_eq!(ui.get_tools_page(), "");
    assert!(!ui.get_tools_over());
    assert_eq!(ui.get_tab(), 2);
    assert_eq!(ui.get_profiles_page(), "extend");

    ui.set_tab(3);
    ui.set_profiles_page("".into());
    ui.invoke_open_tools("scripts".into());
    assert!(!ui.get_tools_over());
    back(&ui);
    assert_eq!(ui.get_tools_page(), "advanced");

    // The stats card's sheet keeps polling and closes before the dashboard is left.
    ui.set_tools_page("".into());
    ui.set_tab(0);
    ui.invoke_open_feed(2);
    assert!(ui.get_feed_open());
    assert!(ui.get_req_visible());
    ui.set_feed_searching(true);
    back(&ui);
    assert!(ui.get_feed_open());
    assert!(!ui.get_feed_searching());
    back(&ui);
    assert!(!ui.get_feed_open());
    assert!(!ui.get_req_visible());
    assert_eq!(ui.get_tab(), 0);
    assert_eq!(*feeds.borrow(), vec![(true, 2), (false, 0)]);

    ui.invoke_open_group_sheet();
    assert!(ui.get_group_sheet_open());
    back(&ui);
    assert!(!ui.get_group_sheet_open());
    assert_eq!(ui.get_tab(), 0);

    // A sheet's grabber or title bar let go: past half its height, or flung down at the
    // release, it closes; flung back up it stays.
    let drag = ui.global::<SheetDrag>();
    assert!(drag.invoke_closes(260.0, 500.0, 0.0));
    assert!(!drag.invoke_closes(200.0, 500.0, 0.0));
    assert!(drag.invoke_closes(120.0, 500.0, 900.0));
    assert!(!drag.invoke_closes(120.0, 500.0, 600.0));
    assert!(!drag.invoke_closes(300.0, 500.0, -900.0));
    assert_eq!(drag.invoke_speed(110.0, 1050, 100.0, 1040, 0.0), 500.0);
    assert_eq!(drag.invoke_speed(110.0, 1060, 100.0, 1050, 500.0), 750.0);
    assert_eq!(drag.invoke_speed(110.0, 1050, 110.0, 1050, 300.0), 300.0);

    // A group's rows unfold on UIKit's outline spring: still at the touch, within a thousandth
    // after 0.367 s, and a turn mid-motion starts from where the block stands at its speed.
    let unfold = ui.global::<UnfoldMotion>();
    assert_eq!(unfold.invoke_at(0.0, 1.0, 0.0, 0.0), 0.0);
    assert_eq!(unfold.invoke_velocity(0.0, 1.0, 0.0, 0.0), 0.0);
    assert!(unfold.invoke_at(0.0, 1.0, 0.0, 0.1) > 0.5);
    assert!(!unfold.invoke_settled(0.0, 1.0, 0.0, 0.3));
    assert!(unfold.invoke_settled(0.0, 1.0, 0.0, 0.37));
    assert!(unfold.invoke_settled(1.0, 0.0, 0.0, 0.37));
    let (x, v) = (
        unfold.invoke_at(0.0, 1.0, 0.0, 0.08),
        unfold.invoke_velocity(0.0, 1.0, 0.0, 0.08),
    );
    assert!(v > 0.0);
    assert!((unfold.invoke_at(x, 0.0, v, 0.0) - x).abs() < 1e-6);
    assert!(unfold.invoke_at(x, 0.0, v, 0.02) > x);
    assert!(unfold.invoke_at(x, 0.0, v, 0.5) < 0.001);

    let reveal = ui.global::<Reveal>();
    assert_eq!(reveal.invoke_offset(100.0, 50.0, 0.0, 400.0, 1000.0), 0.0);
    assert_eq!(
        reveal.invoke_offset(500.0, 50.0, 0.0, 400.0, 1000.0),
        -325.0
    );
    assert_eq!(
        reveal.invoke_offset(950.0, 50.0, 0.0, 400.0, 1000.0),
        -600.0
    );
    assert_eq!(
        reveal.invoke_offset(450.0, 50.0, 300.0, 400.0, 1000.0),
        -300.0
    );

    // `RecordLabelLine` laid out from given widths, so no font is measured.
    let line = ui.global::<RecordLine>();
    let widths = |w: &[f32]| slint::ModelRc::new(slint::VecModel::from(w.to_vec()));
    assert_eq!(line.invoke_before(widths(&[40.0, 30.0]), 2), 78.0);
    let l = line.invoke_layout(200.0, 100.0, 16.0, 20.0, widths(&[40.0, 30.0]), 2);
    assert!(l.text_shown && l.last_shown);
    assert_eq!((l.text_w, l.last_w, l.before_last), (100.0, 30.0, 44.0));
    let l = line.invoke_layout(150.0, 100.0, 16.0, 20.0, widths(&[40.0, 30.0]), 2);
    assert_eq!(l.text_w, 72.0);
    let l = line.invoke_layout(90.0, 100.0, 16.0, 20.0, widths(&[40.0, 30.0]), 2);
    assert!(!l.text_shown && l.last_shown);
    assert_eq!(l.text_w, 0.0);
    let l = line.invoke_layout(100.0, 0.0, 16.0, 20.0, widths(&[40.0, 80.0]), 2);
    assert!(!l.text_shown && l.last_shown);
    assert_eq!(l.last_w, 56.0);
    let l = line.invoke_layout(60.0, 0.0, 16.0, 20.0, widths(&[40.0, 30.0]), 2);
    assert!(!l.last_shown);
    let l = line.invoke_layout(30.0, 20.0, 16.0, 20.0, widths(&[]), 0);
    assert!(l.text_shown && !l.last_shown);
    assert_eq!(l.text_w, 20.0);

    let card = ui.global::<ProxyCardMetrics>();
    for scale in [1.0f32, 1.3] {
        ui.global::<Type>().set_scale(scale);
        let need = 16.0 + 2.0 * card.get_name_line() + 8.0 + card.get_foot_line();
        assert!(card.invoke_height(1) >= need);
        assert!(card.invoke_height(0) >= need - card.get_name_line());
        assert!(card.invoke_height(2) >= need + card.get_caption_line() + 6.0);
    }
    assert_eq!(card.invoke_sticky(40.0, 300.0), 0.0);
    assert_eq!(card.invoke_sticky(-120.0, 300.0), 120.0);
    assert_eq!(card.invoke_sticky(-500.0, 300.0), 300.0);
    assert_eq!(card.invoke_sticky(-10.0, -8.0), 0.0);
}
