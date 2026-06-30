// XXX All the math code was written by AI, I just did some cosmetic changes (naming, structuring,
// inlining etc).

// Using `i64` instead of `usize`, because the intersection math makes intermediate results go
// negative.
const MAX_FILL_DEGREE: i64 = 10;
const MIN_SPLIT_DIMENSION: i64 = 2 * 50;

struct QuadTree {
    state: QuadTreeState,
    cell: QuadTreeCell,
}

struct QuadTreeCell {
    left: i64,
    top: i64,
    // Always a power of two (per `.new()`) so splitting does not result in fractions.
    extent: i64,
}

impl QuadTreeCell {
    fn intersects(&self, line: &Line) -> bool {
        BoundingBox {
            left: self.left,
            top: self.top,
            right: self.left + self.extent,
            bottom: self.top + self.extent,
        }
        .intersect(line)
    }

    fn overlaps(&self, other: &BoundingBox) -> bool {
        BoundingBox {
            left: self.left,
            top: self.top,
            right: self.left + self.extent,
            bottom: self.top + self.extent,
        }
        .overlaps(other)
    }

    fn split_up(&self, old_lines: &[Line]) -> Box<[QuadTree; 4]> {
        let extent = self.extent >> 1;
        let mut result = Box::new([
            QuadTree {
                cell: QuadTreeCell {
                    left: self.left,
                    top: self.top,
                    extent,
                },
                state: QuadTreeState::Leaf {
                    data: Box::new(QuadTreeLeafData { lines: Vec::new() }),
                },
            },
            QuadTree {
                cell: QuadTreeCell {
                    left: self.left + extent,
                    top: self.top,
                    extent,
                },
                state: QuadTreeState::Leaf {
                    data: Box::new(QuadTreeLeafData { lines: Vec::new() }),
                },
            },
            QuadTree {
                cell: QuadTreeCell {
                    left: self.left,
                    top: self.top + extent,
                    extent,
                },
                state: QuadTreeState::Leaf {
                    data: Box::new(QuadTreeLeafData { lines: Vec::new() }),
                },
            },
            QuadTree {
                cell: QuadTreeCell {
                    left: self.left + extent,
                    top: self.top + extent,
                    extent,
                },
                state: QuadTreeState::Leaf {
                    data: Box::new(QuadTreeLeafData { lines: Vec::new() }),
                },
            },
        ]);

        for line in old_lines {
            for child in result.iter_mut() {
                child.insert(line);
            }
        }

        result
    }
}

enum QuadTreeState {
    Leaf { data: Box<QuadTreeLeafData> },
    Parent { children: Box<[QuadTree; 4]> },
}

struct QuadTreeLeafData {
    lines: Vec<Line>,
}

#[derive(Clone, Copy)]
struct Line {
    start: Point,
    end: Point,
}

struct BoundingBox {
    left: i64,
    right: i64,
    top: i64,
    bottom: i64,
}

impl QuadTree {
    pub fn new(min_width: usize, min_height: usize) -> Self {
        Self {
            cell: QuadTreeCell {
                left: 0,
                top: 0,
                extent: min_width.max(min_height).next_power_of_two() as i64,
            },
            state: QuadTreeState::Leaf {
                data: Box::new(QuadTreeLeafData { lines: Vec::new() }),
            },
        }
    }

    pub fn insert(&mut self, line: &Line) {
        if !self.cell.intersects(line) {
            return;
        }

        'leaf: {
            if let QuadTreeState::Leaf { data } = &mut self.state {
                let lines = &mut data.lines;
                if lines.len() as i64 >= MAX_FILL_DEGREE && self.cell.extent >= MIN_SPLIT_DIMENSION
                {
                    // Need to split, too much stuff already.
                    self.state = QuadTreeState::Parent {
                        children: self.cell.split_up(lines),
                    };

                    // Now we are split, so `self` needs to be treated as a parent. This is done
                    // after the 'leaf block.
                    break 'leaf;
                }

                lines.push(line.clone());
                return;
            }
        }

        let QuadTreeState::Parent { children } = &mut self.state else {
            unreachable!()
        };

        for child in children.iter_mut() {
            child.insert(line);
        }
    }

    pub fn intersects_any_line(&self, bounding_box: &BoundingBox) -> bool {
        if !self.cell.overlaps(bounding_box) {
            return false;
        }

        match &self.state {
            QuadTreeState::Leaf { data } => {
                data.lines.iter().any(|line| bounding_box.intersect(line))
            }
            QuadTreeState::Parent { children } => children
                .iter()
                .any(|child| child.intersects_any_line(bounding_box)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i64,
    pub y: i64,
}

fn orientation(a: Point, b: Point, c: Point) -> i64 {
    let bax = b.x - a.x;
    let bay = b.y - a.y;
    let cax = c.x - a.x;
    let cay = c.y - a.y;

    bax * cay - bay * cax
}

fn on_segment(a: Point, b: Point, p: Point, o: &mut i64) -> bool {
    *o = orientation(a, b, p);

    *o == 0
        && p.x >= a.x.min(b.x)
        && p.x <= a.x.max(b.x)
        && p.y >= a.y.min(b.y)
        && p.y <= a.y.max(b.y)
}

pub fn segments_intersect(
    Line { start: a, end: b }: Line,
    Line { start: c, end: d }: Line,
) -> bool {
    let mut o1 = 0;
    let mut o2 = 0;
    let mut o3 = 0;
    let mut o4 = 0;

    if on_segment(a, b, c, &mut o1) {
        return true;
    }
    if on_segment(a, b, d, &mut o2) {
        return true;
    }
    if on_segment(c, d, a, &mut o3) {
        return true;
    }
    if on_segment(c, d, b, &mut o4) {
        return true;
    }

    (o1 > 0) != (o2 > 0) && (o3 > 0) != (o4 > 0)
}

impl BoundingBox {
    fn intersect(&self, line: &Line) -> bool {
        if self.contains(line.start) || self.contains(line.end) {
            return true;
        }

        let bl = Point {
            x: self.left,
            y: self.bottom,
        };
        let br = Point {
            x: self.right,
            y: self.bottom,
        };
        let tr = Point {
            x: self.right,
            y: self.top,
        };
        let tl = Point {
            x: self.left,
            y: self.top,
        };

        segments_intersect(*line, Line { start: bl, end: br })
            || segments_intersect(*line, Line { start: bl, end: tl })
            || segments_intersect(*line, Line { start: tr, end: tl })
            || segments_intersect(*line, Line { start: tr, end: br })
    }

    fn contains(&self, p: Point) -> bool {
        self.left <= p.x && p.x <= self.right && self.top <= p.y && p.y <= self.bottom
    }

    fn overlaps(&self, other: &BoundingBox) -> bool {
        self.left < other.right
            && self.right > other.left
            && self.top < other.bottom
            && self.bottom > other.top
    }
}
