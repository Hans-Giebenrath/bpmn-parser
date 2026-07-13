// XXX All the math code was written by AI, I just did some cosmetic changes (naming, structuring,
// inlining etc).

const CELL_SIZE: u16 = 100;

// It's not _The Grid_, just a grid.
pub struct Grid {
    cells: Box<[CellData]>,
    num_cells_vertically: u16,
    num_cells_horizontally: u16,
}

#[derive(Clone)]
struct WeightedLine {
    line: Line,
    /// Higher is worse.
    weight: u32,
}

struct CellData {
    lines: Vec<WeightedLine>,
}

#[derive(Clone, Copy)]
pub struct Line {
    pub start: Point,
    pub end: Point,
}

impl Line {
    pub fn new(start: &(usize, usize), end: &(usize, usize)) -> Self {
        Self {
            start: Point::from(start),
            end: Point::from(end),
        }
    }
}

pub struct BoundingBox {
    pub left: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
}

impl Grid {
    pub fn new(min_width: usize, min_height: usize) -> Self {
        let num_cells_horizontally = (min_width as u16 + CELL_SIZE - 1) / CELL_SIZE;
        let num_cells_vertically = (min_height as u16 + CELL_SIZE - 1) / CELL_SIZE;
        Self {
            cells: (0..num_cells_vertically * num_cells_horizontally)
                .map(|_| CellData { lines: Vec::new() })
                .collect(),
            num_cells_vertically,
            num_cells_horizontally,
        }
    }

    pub fn insert(&mut self, line: &Line, weight: u32) {
        for index in cells_under_line(line, self.num_cells_vertically) {
            self.cells[index].lines.push(WeightedLine {
                line: line.clone(),
                weight,
            });
        }
    }

    pub fn insert_quadrangle(
        &mut self,
        p1: (usize, usize),
        p2: (usize, usize),
        p3: (usize, usize),
        p4: (usize, usize),
        weight: u32,
    ) {
        self.insert(&Line::new(&p1, &p2), weight);
        self.insert(&Line::new(&p2, &p3), weight);
        self.insert(&Line::new(&p3, &p4), weight);
        self.insert(&Line::new(&p4, &p1), weight);
    }

    pub fn box_intersection_weight(
        &self,
        (x, y): (usize, usize),
        (width, height): (usize, usize),
    ) -> u32 {
        let bounding_box = BoundingBox {
            left: x as i32,
            right: (x + width) as i32,
            top: y as i32,
            bottom: (y + height) as i32,
        };
        self.iterate_indices(&bounding_box)
            .flat_map(|index| &self.cells[index as usize].lines)
            .filter(|wl| bounding_box.intersect(&wl.line))
            .map(|wl| wl.weight)
            .sum()
    }

    pub fn iterate_indices(&self, bounding_box: &BoundingBox) -> impl Iterator<Item = u16> {
        let left = bounding_box.left as u16 / CELL_SIZE;
        let right = bounding_box.right as u16 / CELL_SIZE;
        let top = bounding_box.top as u16 / CELL_SIZE;
        let bottom = bounding_box.bottom as u16 / CELL_SIZE;
        let row_width = self.num_cells_vertically;

        (left..=right)
            .flat_map(move |x| (top..=bottom).map(move |y| (x, y)))
            .map(move |(x, y)| y * row_width + x)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub fn from((x, y): &(usize, usize)) -> Self {
        Self {
            x: *x as i32,
            y: *y as i32,
        }
    }
}

fn orientation(a: Point, b: Point, c: Point) -> i32 {
    let bax = b.x - a.x;
    let bay = b.y - a.y;
    let cax = c.x - a.x;
    let cay = c.y - a.y;

    bax * cay - bay * cax
}

fn on_segment(a: Point, b: Point, p: Point, o: &mut i32) -> bool {
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

// AI helped generating this code. Based on:
// https://www.researchgate.net/publication/2611491_A_Fast_Voxel_Traversal_Algorithm_for_Ray_Tracing
// With help from https://github.com/cgyurgyik/fast-voxel-traversal-algorithm/blob/master/amanatidesWooAlgorithm.cpp
gen fn cells_under_line(
    &Line {
        start: Point { x: x0, y: y0 },
        end: Point { x: x1, y: y1 },
    }: &Line,
    num_cells_vertically: u16,
) -> usize /* indices into Grid::cells */ {
    // cell coordinates, 0, 1, 2, 3, etc
    let mut cx = x0 / CELL_SIZE as i32;
    let mut cy = y0 / CELL_SIZE as i32;
    let end_cx = x1 / CELL_SIZE as i32;
    let end_cy = y1 / CELL_SIZE as i32;

    yield cx as usize + cy as usize * num_cells_vertically as usize;

    let dx = x1 - x0;
    let dy = y1 - y0;

    if dx == 0 && dy == 0 {
        return;
    }

    let step_x = dx.signum();
    let step_y = dy.signum();

    let (t_delta_x, mut t_max_x) = if dx != 0 {
        let next_x = if step_x > 0 {
            (cx + 1) * CELL_SIZE as i32
        } else {
            cx * CELL_SIZE as i32
        };

        (
            CELL_SIZE as f64 / dx.abs() as f64,
            (next_x - x0).abs() as f64 / dx.abs() as f64,
        )
    } else {
        (f64::INFINITY, f64::INFINITY)
    };

    let (t_delta_y, mut t_max_y) = if dy != 0 {
        let next_y = if step_y > 0 {
            (cy + 1) * CELL_SIZE as i32
        } else {
            cy * CELL_SIZE as i32
        };

        (
            CELL_SIZE as f64 / dy.abs() as f64,
            (next_y - y0).abs() as f64 / dy.abs() as f64,
        )
    } else {
        (f64::INFINITY, f64::INFINITY)
    };

    while cx != end_cx || cy != end_cy {
        if t_max_x < t_max_y {
            cx += step_x;
            t_max_x += t_delta_x;
        } else {
            cy += step_y;
            t_max_y += t_delta_y;
        }

        yield cx as usize + cy as usize * num_cells_vertically as usize;
    }
}
