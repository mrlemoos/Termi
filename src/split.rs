//! Split layout: a binary tree of tab ids. Each screen is one tree; every tab lives in exactly one.

use eframe::egui::{Rect, pos2};

#[derive(Clone, Debug, PartialEq)]
pub enum Layout {
    Leaf(u64),
    /// `side_by_side`: left | right, else top / bottom.
    Split { side_by_side: bool, a: Box<Layout>, b: Box<Layout> },
}

impl Layout {
    pub fn contains(&self, id: u64) -> bool {
        match self {
            Layout::Leaf(l) => *l == id,
            Layout::Split { a, b, .. } => a.contains(id) || b.contains(id),
        }
    }

    /// Replace leaf `at` with `at` + `new`, new one right of / below it.
    pub fn split(self, at: u64, new: u64, side_by_side: bool) -> Layout {
        match self {
            Layout::Leaf(l) if l == at => Layout::Split { side_by_side, a: Box::new(Layout::Leaf(l)), b: Box::new(Layout::Leaf(new)) },
            Layout::Split { side_by_side: s, a, b } => Layout::Split { side_by_side: s, a: Box::new(a.split(at, new, side_by_side)), b: Box::new(b.split(at, new, side_by_side)) },
            leaf => leaf,
        }
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

    /// Each leaf's rect, halves split at `snap` multiples so panes stay grid-aligned.
    pub fn rects(&self, r: Rect, snap: (f32, f32)) -> Vec<(u64, Rect)> {
        match self {
            Layout::Leaf(l) => vec![(*l, r)],
            Layout::Split { side_by_side: true, a, b } => {
                let x = r.min.x + (r.width() / 2.0 / snap.0).round() * snap.0;
                let mut v = a.rects(Rect::from_min_max(r.min, pos2(x, r.max.y)), snap);
                v.extend(b.rects(Rect::from_min_max(pos2(x, r.min.y), r.max), snap));
                v
            }
            Layout::Split { a, b, .. } => {
                let y = r.min.y + (r.height() / 2.0 / snap.1).round() * snap.1;
                let mut v = a.rects(Rect::from_min_max(r.min, pos2(r.max.x, y)), snap);
                v.extend(b.rects(Rect::from_min_max(pos2(r.min.x, y), r.max), snap));
                v
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_retain_rects() {
        let l = Layout::Leaf(1).split(1, 2, true).split(2, 3, false);
        assert!(l.contains(3) && !l.contains(4));
        let r = Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0));
        let ids: Vec<_> = l.rects(r, (1.0, 1.0)).into_iter().map(|(id, r)| (id, r.min.x, r.min.y)).collect();
        assert_eq!(ids, [(1, 0.0, 0.0), (2, 50.0, 0.0), (3, 50.0, 50.0)]);
        assert_eq!(l.clone().retain(&|id| id != 2), Some(Layout::Leaf(1).split(1, 3, true)));
        assert_eq!(l.retain(&|_| false), None);
    }
}
