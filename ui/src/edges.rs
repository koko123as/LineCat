//! The window's own insets from `WindowEdges.kt`, in physical pixels: Slint's safe area is the
//! whole screen's, wrong in Flyme's small window.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Insets {
    pub fn from_json(v: &serde_json::Value) -> Insets {
        let px = |k: &str| {
            let n = v.get(k).and_then(serde_json::Value::as_f64).unwrap_or(0.0) as f32;
            if n.is_finite() {
                n.max(0.0)
            } else {
                0.0
            }
        };
        Insets {
            left: px("left"),
            top: px("top"),
            right: px("right"),
            bottom: px("bottom"),
        }
    }

    pub fn scaled(self, scale: f32) -> Insets {
        let scale = scale.max(0.1);
        Insets {
            left: self.left / scale,
            top: self.top / scale,
            right: self.right / scale,
            bottom: self.bottom / scale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_shell_event() {
        let v =
            serde_json::json!({"ev": "insets", "top": 120, "bottom": 96, "left": 0, "right": 8});
        let i = Insets::from_json(&v);
        assert_eq!((i.left, i.top, i.right, i.bottom), (0.0, 120.0, 8.0, 96.0));
    }

    #[test]
    fn missing_or_negative_values_are_zero() {
        let i = Insets::from_json(&serde_json::json!({"top": -4, "bottom": "x"}));
        assert_eq!(i, Insets::default());
    }

    #[test]
    fn scales_to_logical_pixels() {
        let i = Insets {
            left: 0.0,
            top: 90.0,
            right: 30.0,
            bottom: 63.0,
        }
        .scaled(3.0);
        assert_eq!((i.top, i.right, i.bottom), (30.0, 10.0, 21.0));
        assert_eq!(Insets::default().scaled(0.0), Insets::default());
    }
}
