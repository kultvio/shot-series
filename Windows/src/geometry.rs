pub const MINIMUM_AREA: i32 = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self { left, top, right, bottom }
    }

    pub fn width(self) -> i32 { self.right.saturating_sub(self.left) }

    pub fn height(self) -> i32 { self.bottom.saturating_sub(self.top) }

    pub fn is_selectable(self) -> bool {
        self.width() >= MINIMUM_AREA && self.height() >= MINIMUM_AREA
    }

    pub fn offset(self, dx: i32, dy: i32) -> Self {
        Self::new(self.left + dx, self.top + dy, self.right + dx, self.bottom + dy)
    }

    pub fn intersection(self, bounds: Self) -> Self {
        Self::new(self.left.max(bounds.left), self.top.max(bounds.top),
                  self.right.min(bounds.right), self.bottom.min(bounds.bottom))
    }

    pub fn fit(self, bounds: Self) -> Option<Self> {
        if !self.is_selectable() || !bounds.is_selectable() { return None; }
        let width = self.width().min(bounds.width());
        let height = self.height().min(bounds.height());
        let left = self.left.clamp(0, bounds.width() - width) + bounds.left;
        let top = self.top.clamp(0, bounds.height() - height) + bounds.top;
        Some(Self::new(left, top, left + width, top + height))
    }

    pub fn between(start: Point, end: Point, bounds: Self) -> Self {
        let (a, b) = (start.clamped(bounds), end.clamped(bounds));
        Self::new(a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y))
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Point { pub x: i32, pub y: i32 }

impl Point {
    pub fn clamped(self, limits: Rect) -> Self {
        Self { x: self.x.clamp(limits.left, limits.right), y: self.y.clamp(limits.top, limits.bottom) }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Edge { TopLeft, Top, TopRight, Left, Right, BottomLeft, Bottom, BottomRight }

impl Edge {
    pub const ALL: [Self; 8] = [Self::TopLeft, Self::Top, Self::TopRight, Self::Left, Self::Right,
                                Self::BottomLeft, Self::Bottom, Self::BottomRight];

    pub fn direction(self) -> (i32, i32) {
        match self {
            Self::TopLeft => (-1, -1), Self::Top => (0, -1), Self::TopRight => (1, -1),
            Self::Left => (-1, 0), Self::Right => (1, 0),
            Self::BottomLeft => (-1, 1), Self::Bottom => (0, 1), Self::BottomRight => (1, 1),
        }
    }

    pub fn anchor(self, area: Rect) -> Point {
        let (x, y) = self.direction();
        Point {
            x: area.left + (x + 1) * area.width() / 2,
            y: area.top + (y + 1) * area.height() / 2,
        }
    }

    pub fn hit_rect(self, area: Rect, limits: Rect, radius: i32) -> Rect {
        let (dx, dy) = self.direction();
        let center = self.anchor(area);
        let rx = radius.min(area.width() / 3);
        let ry = radius.min(area.height() / 3);
        let mut hit = Rect::new(center.x - rx, center.y - ry, center.x + rx, center.y + ry);
        if dx == 0 { hit.left = area.left + rx; hit.right = area.right - rx; }
        if dy == 0 { hit.top = area.top + ry; hit.bottom = area.bottom - ry; }
        hit.intersection(limits)
    }

    pub fn resize(self, mut area: Rect, delta: Point, limits: Rect) -> Rect {
        let (dx, dy) = self.direction();
        if dx < 0 { area.left = (area.left + delta.x).clamp(limits.left, area.right - MINIMUM_AREA); }
        if dx > 0 { area.right = (area.right + delta.x).clamp(area.left + MINIMUM_AREA, limits.right); }
        if dy < 0 { area.top = (area.top + delta.y).clamp(limits.top, area.bottom - MINIMUM_AREA); }
        if dy > 0 { area.bottom = (area.bottom + delta.y).clamp(area.top + MINIMUM_AREA, limits.bottom); }
        area
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_and_handles_on_left_monitor() {
        let bounds = Rect::new(-1920, 0, 0, 1080);
        let area = Rect::between(Point { x: -600, y: 500 }, Point { x: -1200, y: 70 }, bounds);
        assert_eq!(area, Rect::new(-1200, 70, -600, 500));
        assert_eq!(area.offset(1920, 0).fit(bounds), Some(area));
        for edge in Edge::ALL {
            for delta in [Point { x: 9000, y: -9000 }, Point { x: -9000, y: 9000 }] {
                let resized = edge.resize(area, delta, bounds);
                assert!(resized.is_selectable());
                assert_eq!(resized.intersection(bounds), resized);
                let hit = edge.hit_rect(resized, bounds, 16);
                assert!(hit.width() > 0 && hit.height() > 0);
                assert_eq!(hit.intersection(bounds), hit);
            }
        }
    }
}
