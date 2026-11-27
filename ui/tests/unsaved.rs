//! The profile edit page and its file editor leave through Rust, which asks first when
//! something is unsaved.
use seasoncat_ui::*;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Key, Platform, WindowAdapter, WindowEvent};
use std::cell::RefCell;
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
fn back_asks_before_leaving_unsaved_profile_edits() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
    window.set_size(slint::PhysicalSize::new(412, 892));
    let ui = AppWindow::new().unwrap();
    let asked = Rc::new(RefCell::new(Vec::<String>::new()));
    let a = asked.clone();
    ui.on_edit_back(move || a.borrow_mut().push("edit".into()));
    let a = asked.clone();
    ui.on_profile_editor_back(move |text| a.borrow_mut().push(text.into()));
    ui.show().unwrap();

    ui.set_tab(2);
    ui.set_profiles_page("editor".into());
    ui.set_editor_text("mode: rule".into());
    back(&ui);
    assert_eq!(ui.get_profiles_page(), "editor");
    ui.set_profiles_page("edit".into());
    back(&ui);
    assert_eq!(ui.get_profiles_page(), "edit");
    assert_eq!(*asked.borrow(), ["mode: rule", "edit"]);

    ui.set_editor_parent("rules".into());
    ui.set_profiles_page("editor".into());
    back(&ui);
    assert_eq!(asked.borrow().len(), 2);
}
