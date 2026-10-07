//! Split layout: a binary tree of tab ids. Each screen is one tree; every tab lives in exactly one.

use eframe::egui::{Rect, pos2};

#[derive(Clone, Debug, PartialEq)]
pub enum Layout {
    Leaf(u64),
    /// `side_by_side`: left | right, else top / bottom.
    Split { side_by_side: bool, a: Box<Layout>, b: Box<Layout> },
}

/// Halve `r`, snapped to `snap` multiples so panes stay grid-aligned.
fn halves(r: Rect, side_by_side: bool, snap: (f32, f32)) -> (Rect, Rect) {
    if side_by_side {
        let x = r.min.x + (r.width() / 2.0 / snap.0).round() * snap.0;
        (Rect::from_min_max(r.min, pos2(x, r.max.y)), Rect::from_min_max(pos2(x, r.min.y), r.max))
    } else {
        let y = r.min.y + (r.height() / 2.0 / snap.1).round() * snap.1;
        (Rect::from_min_max(r.min, pos2(r.max.x, y)), Rect::from_min_max(pos2(r.min.x, y), r.max))
    }
}

impl Layout {
    pub fn contains(&self, id: u64) -> bool {
        match self {
            Layout::Leaf(l) => *l == id,
            Layout::Split { a, b, .. } => a.contains(id) || b.contains(id),
        }
    }

    /// Replace leaf `at` with `at` + `new`, new one right of / below it (left of / above if `before`).
    pub fn split(self, at: u64, new: u64, side_by_side: bool, before: bool) -> Layout {
        match self {
            Layout::Leaf(l) if l == at => {
                let (a, b) = if before { (new, l) } else { (l, new) };
                Layout::Split { side_by_side, a: Box::new(Layout::Leaf(a)), b: Box::new(Layout::Leaf(b)) }
            }
            Layout::Split { side_by_side: s, a, b } => {
                Layout::Split { side_by_side: s, a: Box::new(a.split(at, new, side_by_side, before)), b: Box::new(b.split(at, new, side_by_side, before)) }
            }
            leaf => leaf,
        }
    }

    /// Take `src` out and re-split it next to `dst`.
    pub fn moved(self, src: u64, dst: u64, side_by_side: bool, before: bool) -> Layout {
        self.retain(&|id| id != src).unwrap_or(Layout::Leaf(dst)).split(dst, src, side_by_side, before)
    }

    /// Drop leaves failing `keep`; a split left with one child collapses into it.
    pub fn retain(self, keep: &impl Fn(u64) -> bool) -> Option<Layout> {
        match self {
            Layout::Leaf(l) => keep(l).then_some(Layout::Leaf(l)),
            Layout::Split { side_by_side, a, b } => match (a.retain(keep), b.retain(keep)) {
                (Some(a), Some(b)) => Some(Layout::Split { side_by_side, a: Box::new(a), b: Box::new(b) }),
                (one, other) => one.or(other),
            },
        }
    }

    /// Each leaf's rect.
    pub fn rects(&self, r: Rect, snap: (f32, f32)) -> Vec<(u64, Rect)> {
        match self {
            Layout::Leaf(l) => vec![(*l, r)],
            Layout::Split { side_by_side, a, b } => {
                let (ra, rb) = halves(r, *side_by_side, snap);
                let mut v = a.rects(ra, snap);
                v.extend(b.rects(rb, snap));
                v
            }
        }
    }

    /// Each split's dividing line, as (start, end).
    pub fn dividers(&self, r: Rect, snap: (f32, f32)) -> Vec<[eframe::egui::Pos2; 2]> {
        let Layout::Split { side_by_side, a, b } = self else { return Vec::new() };
        let (ra, rb) = halves(r, *side_by_side, snap);
        let mut v = vec![[rb.min, if *side_by_side { rb.left_bottom() } else { rb.right_top() }]];
        v.extend(a.dividers(ra, snap));
        v.extend(b.dividers(rb, snap));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_move_retain_rects() {
        let l = Layout::Leaf(1).split(1, 2, true, false).split(2, 3, false, false);
        assert!(l.contains(3) && !l.contains(4));
        let r = Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0));
        let origins = |l: &Layout| l.rects(r, (1.0, 1.0)).into_iter().map(|(id, r)| (id, r.min.x, r.min.y)).collect::<Vec<_>>();
        assert_eq!(origins(&l), [(1, 0.0, 0.0), (2, 50.0, 0.0), (3, 50.0, 50.0)]);
        assert_eq!(l.dividers(r, (1.0, 1.0)), [[pos2(50.0, 0.0), pos2(50.0, 100.0)], [pos2(50.0, 50.0), pos2(100.0, 50.0)]]);
        // 3 to the left of 1: 2 takes the whole right column
        assert_eq!(origins(&l.clone().moved(3, 1, true, true)), [(3, 0.0, 0.0), (1, 25.0, 0.0), (2, 50.0, 0.0)]);
        assert_eq!(l.clone().retain(&|id| id != 2).map(|l| origins(&l)), Some(vec![(1, 0.0, 0.0), (3, 50.0, 0.0)]));
        assert_eq!(l.retain(&|_| false), None);
    }
}
