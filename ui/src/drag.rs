//! Where a lifted card lands among cards of unequal heights: a card makes way once the
//! dragged card's leading edge passes its middle.

pub fn tops(heights: &[f32], gap: f32) -> Vec<f32> {
    let mut y = 0.0;
    heights
        .iter()
        .map(|h| {
            let top = y;
            y += h.max(0.0) + gap;
            top
        })
        .collect()
}

pub fn target(heights: &[f32], gap: f32, from: usize, shift: f32) -> usize {
    if from >= heights.len() || !shift.is_finite() {
        return from.min(heights.len().saturating_sub(1));
    }
    let tops = tops(heights, gap);
    let mid = |j: usize| tops[j] + heights[j].max(0.0) / 2.0;
    let mut to = from;
    if shift > 0.0 {
        let bottom = tops[from] + heights[from].max(0.0) + shift;
        for j in from + 1..heights.len() {
            if bottom <= mid(j) {
                break;
            }
            to = j;
        }
    } else if shift < 0.0 {
        let top = tops[from] + shift;
        for j in (0..from).rev() {
            if top >= mid(j) {
                break;
            }
            to = j;
        }
    }
    to
}

pub fn offset(heights: &[f32], gap: f32, from: usize, to: usize, index: usize) -> f32 {
    let Some(h) = heights.get(from) else {
        return 0.0;
    };
    let room = h.max(0.0) + gap;
    if index > from && index <= to {
        -room
    } else if index < from && index >= to {
        room
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: [f32; 4] = [200.0, 100.0, 300.0, 60.0];

    #[test]
    fn stays_until_the_neighbours_middle() {
        assert_eq!(target(&H, 8.0, 0, 0.0), 0);
        assert_eq!(target(&H, 8.0, 0, 57.0), 0);
        assert_eq!(target(&H, 8.0, 0, 59.0), 1);
    }

    #[test]
    fn a_tall_neighbour_needs_more_travel() {
        assert_eq!(target(&H, 8.0, 1, 157.0), 1);
        assert_eq!(target(&H, 8.0, 1, 159.0), 2);
        assert_eq!(target(&H, 8.0, 1, 347.0), 3);
        assert_eq!(target(&H, 8.0, 1, 5000.0), 3);
    }

    #[test]
    fn moving_up_uses_the_top_edge() {
        assert_eq!(target(&H, 8.0, 3, -157.0), 3);
        assert_eq!(target(&H, 8.0, 3, -159.0), 2);
        assert_eq!(target(&H, 8.0, 3, -367.0), 1);
        assert_eq!(target(&H, 8.0, 3, -525.0), 0);
        assert_eq!(target(&H, 8.0, 3, -9000.0), 0);
    }

    #[test]
    fn neighbours_make_room_for_the_dragged_height() {
        assert_eq!(offset(&H, 8.0, 0, 2, 1), -208.0);
        assert_eq!(offset(&H, 8.0, 0, 2, 2), -208.0);
        assert_eq!(offset(&H, 8.0, 0, 2, 3), 0.0);
        assert_eq!(offset(&H, 8.0, 3, 1, 1), 68.0);
        assert_eq!(offset(&H, 8.0, 3, 1, 2), 68.0);
        assert_eq!(offset(&H, 8.0, 3, 1, 0), 0.0);
        assert_eq!(offset(&H, 8.0, 3, 1, 3), 0.0);
    }

    #[test]
    fn bad_input_keeps_the_card() {
        assert_eq!(target(&[], 8.0, 0, 50.0), 0);
        assert_eq!(target(&H, 8.0, 9, 50.0), 3);
        assert_eq!(target(&H, 8.0, 1, f32::NAN), 1);
        assert_eq!(offset(&H, 8.0, 9, 0, 0), 0.0);
        assert_eq!(tops(&H, 8.0), [0.0, 208.0, 316.0, 624.0]);
    }
}
