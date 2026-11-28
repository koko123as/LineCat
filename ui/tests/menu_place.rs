//! `MenuPlace`, as prism's `belowPoint`: from the press rightwards, above a held row, else
//! below it, else under the press (over it near the bottom), 16 inside the window.
use seasoncat_ui::*;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use std::rc::Rc;

struct Headless(Rc<MinimalSoftwareWindow>);

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

#[test]
fn menu_place() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
    let ui = AppWindow::new().unwrap();
    let place = ui.global::<MenuPlace>();

    assert_eq!(place.invoke_left_of(100.0, 200.0, 400.0), 100.0);
    assert_eq!(place.invoke_left_of(300.0, 200.0, 400.0), 184.0);
    assert_eq!(place.invoke_left_of(4.0, 200.0, 400.0), 16.0);

    assert_eq!(
        place.invoke_top_of(530.0, 500.0, 560.0, true, 120.0, 900.0),
        372.0
    );
    assert_eq!(
        place.invoke_top_of(80.0, 50.0, 110.0, true, 120.0, 900.0),
        118.0
    );
    assert_eq!(
        place.invoke_top_of(530.0, 500.0, 560.0, false, 120.0, 900.0),
        530.0
    );
    assert_eq!(
        place.invoke_top_of(850.0, 820.0, 880.0, false, 120.0, 900.0),
        730.0
    );
    assert_eq!(
        place.invoke_top_of(300.0, 280.0, 340.0, true, 1000.0, 900.0),
        16.0
    );
}
