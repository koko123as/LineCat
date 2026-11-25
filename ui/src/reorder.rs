//! `copyAndReorder`, where a reorderable list's drop lands.

pub fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize) {
    if from >= items.len() || to >= items.len() || from == to {
        return;
    }
    let item = items.remove(from);
    items.insert(to, item);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_down_and_up() {
        let mut v = vec!['a', 'b', 'c', 'd'];
        move_item(&mut v, 0, 2);
        assert_eq!(v, ['b', 'c', 'a', 'd']);
        move_item(&mut v, 3, 0);
        assert_eq!(v, ['d', 'b', 'c', 'a']);
    }

    #[test]
    fn ignores_out_of_range() {
        let mut v = vec![1, 2];
        move_item(&mut v, 0, 5);
        move_item(&mut v, 4, 0);
        assert_eq!(v, [1, 2]);
    }
}
