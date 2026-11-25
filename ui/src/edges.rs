//! Where the window's visible area sits in the surface Slint draws to.
//!
//! A NativeActivity draws its own surface from the top left, and Slint lays out at the
//! surface's size. In a freeform window (Flyme's small window) that surface can be larger
//! than the window the system shows: the window manager adds surface insets around a
//! floating window, and the part outside the window frame is never composited. The shell
//! (`WindowEdges.kt`) reports the decor view's size, the bars it reaches into and, when it
//! can read them, the surface insets; what is left over is padding around the content.

/// Padding, in physical pixels, from each edge of the surface to the content.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// What `WindowEdges.kt` measured, in physical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Report {
    /// The bars (status, navigation, cutout) as far as the window reaches into them.
    pub bars_top: f32,
    pub bars_bottom: f32,
    /// The decor view's size: the window as the system shows it.
    pub width: f32,
    pub height: f32,
    /// `LayoutParams.surfaceInsets`, when the shell could read them.
    pub surface: Option<Insets>,
}

impl Report {
    pub fn from_json(v: &serde_json::Value) -> Report {
        let px = |k: &str| v.get(k).and_then(serde_json::Value::as_f64).unwrap_or(0.0) as f32;
        let surface = v.get("surface").filter(|s| s.is_object()).map(|s| {
            let px = |k: &str| s.get(k).and_then(serde_json::Value::as_f64).unwrap_or(0.0) as f32;
            Insets {
                left: px("left"),
                top: px("top"),
                right: px("right"),
                bottom: px("bottom"),
            }
        });
        Report {
            bars_top: px("top"),
            bars_bottom: px("bottom"),
            width: px("width"),
            height: px("height"),
            surface,
        }
    }
}

/// The padding for a surface of `surface` physical pixels.
pub fn fit(surface: (f32, f32), report: &Report) -> Insets {
    let mut pad = Insets::default();
    if report.width >= 1.0 && report.height >= 1.0 {
        let extra_w = (surface.0 - report.width).max(0.0);
        let extra_h = (surface.1 - report.height).max(0.0);
        let reported = report.surface.filter(|s| {
            (s.left + s.right - extra_w).abs() <= 2.0 && (s.top + s.bottom - extra_h).abs() <= 2.0
        });
        pad = match reported {
            Some(s) => s,
            // Surface insets of a floating window go round all four sides alike; a surface
            // only taller than its window has its extra at the bottom.
            None if extra_w >= 1.0 => Insets {
                left: (extra_w / 2.0).floor(),
                right: extra_w - (extra_w / 2.0).floor(),
                top: (extra_h / 2.0).floor(),
                bottom: extra_h - (extra_h / 2.0).floor(),
            },
            None => Insets {
                bottom: extra_h,
                ..Insets::default()
            },
        };
    }
    pad.top += report.bars_top.max(0.0);
    pad.bottom += report.bars_bottom.max(0.0);
    pad
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(width: f32, height: f32) -> Report {
        Report {
            bars_top: 0.0,
            bars_bottom: 0.0,
            width,
            height,
            surface: None,
        }
    }

    #[test]
    fn a_full_screen_window_keeps_only_its_bars() {
        let r = Report {
            bars_top: 120.0,
            bars_bottom: 48.0,
            ..report(1440.0, 3200.0)
        };
        let pad = fit((1440.0, 3200.0), &r);
        assert_eq!(
            (pad.left, pad.top, pad.right, pad.bottom),
            (0.0, 120.0, 0.0, 48.0)
        );
    }

    #[test]
    fn a_taller_surface_pads_the_bottom() {
        let pad = fit((780.0, 1450.0), &report(780.0, 1350.0));
        assert_eq!(pad.bottom, 100.0);
        assert_eq!((pad.left, pad.top, pad.right), (0.0, 0.0, 0.0));
    }

    #[test]
    fn surface_insets_on_every_side_split_evenly() {
        let pad = fit((860.0, 1431.0), &report(780.0, 1350.0));
        assert_eq!((pad.left, pad.right), (40.0, 40.0));
        assert_eq!((pad.top, pad.bottom), (40.0, 41.0));
    }

    #[test]
    fn reported_surface_insets_win_when_they_add_up() {
        let mut r = report(780.0, 1350.0);
        r.bars_top = 10.0;
        r.surface = Some(Insets {
            left: 20.0,
            top: 10.0,
            right: 20.0,
            bottom: 70.0,
        });
        let pad = fit((820.0, 1430.0), &r);
        assert_eq!(
            (pad.left, pad.top, pad.right, pad.bottom),
            (20.0, 20.0, 20.0, 70.0)
        );
        let pad = fit((820.0, 1350.0), &r);
        assert_eq!((pad.left, pad.top, pad.bottom), (20.0, 10.0, 0.0));
    }

    #[test]
    fn no_report_or_a_larger_window_adds_nothing() {
        assert_eq!(fit((400.0, 800.0), &report(0.0, 0.0)), Insets::default());
        assert_eq!(
            fit((400.0, 800.0), &report(500.0, 900.0)),
            Insets::default()
        );
    }

    #[test]
    fn reads_the_shell_event() {
        let v = serde_json::json!({"ev": "insets", "top": 5, "bottom": 7, "width": 300, "height": 500,
            "surface": {"left": 1, "top": 2, "right": 3, "bottom": 4}});
        let r = Report::from_json(&v);
        assert_eq!(
            (r.bars_top, r.bars_bottom, r.width, r.height),
            (5.0, 7.0, 300.0, 500.0)
        );
        assert_eq!(r.surface.unwrap().right, 3.0);
        assert!(Report::from_json(&serde_json::json!({"top": 1}))
            .surface
            .is_none());
    }
}
