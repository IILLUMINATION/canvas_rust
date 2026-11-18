//! Uniform grid spatial index for nodes and edges.
//!
//! Both indexes are CSR (counting sort) arrays: no per-cell `Vec`, no hashing,
//! no allocation in the query path. Queries walk only the cells overlapping the
//! viewport.
//!
//! Mutation model:
//!
//! * topology change -> `topo_dirty`, full O(n+m) rebuild on next query;
//! * node move -> node pushed to `dirty_nodes`, its incident edges to
//!   `dirty_edges`. Queries union the grid result with the dirty lists, so a
//!   moved node can never be culled incorrectly. A full rebuild happens only
//!   once the dirty list grows past `dirty_limit`.

use crate::graph::Edge;

pub const NO: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Rect {
    pub const EMPTY: Rect = Rect {
        x0: f32::MAX,
        y0: f32::MAX,
        x1: f32::MIN,
        y1: f32::MIN,
    };

    #[inline]
    pub fn from_center(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect {
            x0: x - w * 0.5,
            y0: y - h * 0.5,
            x1: x + w * 0.5,
            y1: y + h * 0.5,
        }
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.x0 <= self.x1 && self.y0 <= self.y1
    }

    #[inline]
    pub fn extend(&mut self, x: f32, y: f32) {
        if x < self.x0 {
            self.x0 = x;
        }
        if x > self.x1 {
            self.x1 = x;
        }
        if y < self.y0 {
            self.y0 = y;
        }
        if y > self.y1 {
            self.y1 = y;
        }
    }

    #[inline]
    pub fn intersects(&self, o: &Rect) -> bool {
        self.x0 <= o.x1 && self.x1 >= o.x0 && self.y0 <= o.y1 && self.y1 >= o.y0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Grid {
    pub cell: f32,
    pub inv: f32,
    pub ox: f32,
    pub oy: f32,
    pub gw: u32,
    pub gh: u32,
}

impl Grid {
    pub const EMPTY: Grid = Grid {
        cell: 1.0,
        inv: 1.0,
        ox: 0.0,
        oy: 0.0,
        gw: 1,
        gh: 1,
    };

    /// Cover `bbox` with roughly `target_cells` cells, but never with cells
    /// smaller than `min_cell` (keeps per-node cell spans tiny) and never with
    /// more than 4096 cells per axis (bounds memory on huge worlds).
    pub fn for_bbox(b: Rect, target_cells: u32, min_cell: f32) -> Grid {
        let w = (b.x1 - b.x0).max(1.0);
        let h = (b.y1 - b.y0).max(1.0);
        let c = (w * h / target_cells.max(1) as f32)
            .sqrt()
            .max(min_cell)
            .max(1.0);
        let gw = ((w / c).ceil() as u32).saturating_add(1).clamp(1, 4096);
        let gh = ((h / c).ceil() as u32).saturating_add(1).clamp(1, 4096);
        Grid {
            cell: c,
            inv: 1.0 / c,
            ox: b.x0,
            oy: b.y0,
            gw,
            gh,
        }
    }

    #[inline]
    pub fn ix(&self, x: f32) -> u32 {
        let v = ((x - self.ox) * self.inv) as i32;
        v.clamp(0, self.gw as i32 - 1) as u32
    }

    #[inline]
    pub fn iy(&self, y: f32) -> u32 {
        let v = ((y - self.oy) * self.inv) as i32;
        v.clamp(0, self.gh as i32 - 1) as u32
    }

    #[inline]
    pub fn cells(&self) -> usize {
        self.gw as usize * self.gh as usize
    }

    #[inline]
    fn idx(&self, cx: u32, cy: u32) -> usize {
        cy as usize * self.gw as usize + cx as usize
    }
}

pub struct Index {
    pub node_grid: Grid,
    pub node_starts: Vec<u32>,
    pub node_items: Vec<u32>,

    pub edge_grid: Grid,
    pub edge_starts: Vec<u32>,
    pub edge_items: Vec<u32>,
    pub long_edges: Vec<u32>,

    /// node -> incident edge ids (CSR), rebuilt on topology change.
    pub adj_start: Vec<u32>,
    pub adj: Vec<u32>,

    pub dirty_nodes: Vec<u32>,
    pub dirty_edges: Vec<u32>,
    pub dirty_limit: u32,

    pub topo_dirty: bool,
    pub bounds: Rect,
}

impl Default for Index {
    fn default() -> Self {
        Index {
            node_grid: Grid::EMPTY,
            node_starts: Vec::new(),
            node_items: Vec::new(),
            edge_grid: Grid::EMPTY,
            edge_starts: Vec::new(),
            edge_items: Vec::new(),
            long_edges: Vec::new(),
            adj_start: Vec::new(),
            adj: Vec::new(),
            dirty_nodes: Vec::new(),
            dirty_edges: Vec::new(),
            dirty_limit: 256,
            topo_dirty: true,
            bounds: Rect::EMPTY,
        }
    }
}

/// Cell range [lo, hi] (inclusive) for a bbox, clamped to the grid.
#[inline]
fn span(g: &Grid, r: &Rect) -> (u32, u32, u32, u32) {
    (g.ix(r.x0), g.ix(r.x1), g.iy(r.y0), g.iy(r.y1))
}

impl Index {
    pub fn rebuild(&mut self, x: &[f32], y: &[f32], w: &[f32], h: &[f32], edges: &[Edge]) {
        let n = x.len();
        self.topo_dirty = false;
        self.dirty_nodes.clear();
        self.dirty_edges.clear();

        // ---- world bounds ----
        let mut b = Rect::EMPTY;
        for i in 0..n {
            b.extend(x[i] - w[i] * 0.5, y[i] - h[i] * 0.5);
            b.extend(x[i] + w[i] * 0.5, y[i] + h[i] * 0.5);
        }
        if !b.is_valid() {
            b = Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 1.0,
                y1: 1.0,
            };
        }
        self.bounds = b;
        self.dirty_limit = (n as u32 / 16).max(256);

        // ---- nodes ----
        let mut min_cell = 32.0f32;
        for i in 0..n.min(2048) {
            min_cell = min_cell.max(w[i].min(h[i]));
        }
        let grid = Grid::for_bbox(b, n.max(1) as u32, min_cell);
        let cells = grid.cells();
        self.node_grid = grid;
        self.node_starts.clear();
        self.node_starts.resize(cells + 1, 0);
        for i in 0..n {
            let r = Rect::from_center(x[i], y[i], w[i], h[i]);
            let (cx0, cx1, cy0, cy1) = span(&grid, &r);
            for cy in cy0..=cy1 {
                let row = cy as usize * grid.gw as usize;
                for cx in cx0..=cx1 {
                    self.node_starts[row + cx as usize + 1] += 1;
                }
            }
        }
        for i in 0..cells {
            self.node_starts[i + 1] += self.node_starts[i];
        }
        // scatter using a cursor copied from starts
        let mut cursor: Vec<u32> = self.node_starts[..cells].to_vec();
        self.node_items.clear();
        self.node_items.resize(self.node_starts[cells] as usize, 0);
        for i in 0..n {
            let r = Rect::from_center(x[i], y[i], w[i], h[i]);
            let (cx0, cx1, cy0, cy1) = span(&grid, &r);
            for cy in cy0..=cy1 {
                let row = cy as usize * grid.gw as usize;
                for cx in cx0..=cx1 {
                    let c = row + cx as usize;
                    self.node_items[cursor[c] as usize] = i as u32;
                    cursor[c] += 1;
                }
            }
        }

        // ---- adjacency ----
        let m = edges.len();
        self.adj_start.clear();
        self.adj_start.resize(n + 1, 0);
        for e in edges {
            self.adj_start[e.a as usize + 1] += 1;
            self.adj_start[e.b as usize + 1] += 1;
        }
        for i in 0..n {
            self.adj_start[i + 1] += self.adj_start[i];
        }
        self.adj.clear();
        self.adj.resize(self.adj_start[n] as usize, 0);
        let mut cur2: Vec<u32> = self.adj_start[..n].to_vec();
        for (ei, e) in edges.iter().enumerate() {
            let (a, b) = (e.a as usize, e.b as usize);
            self.adj[cur2[a] as usize] = ei as u32;
            cur2[a] += 1;
            self.adj[cur2[b] as usize] = ei as u32;
            cur2[b] += 1;
        }

        // ---- edges ----
        // Cell side ~ median edge length: then most edges live in 1-2 cells.
        // Edges that still span more than 2x2 cells go to `long_edges` and are
        // bbox-tested in one linear pass per frame (sequential, cache friendly).
        let mut med = min_cell;
        if m > 0 {
            let stride = (m / 2048).max(1);
            let mut lens: Vec<f32> = Vec::with_capacity(2048);
            let mut i = 0;
            while i < m && lens.len() < 2048 {
                let e = edges[i];
                let (ax, ay) = (x[e.a as usize], y[e.a as usize]);
                let (bx, by) = (x[e.b as usize], y[e.b as usize]);
                lens.push((bx - ax).hypot(by - ay));
                i += stride;
            }
            lens.sort_by(|a, b| a.partial_cmp(b).unwrap());
            med = lens[lens.len() / 2].max(min_cell);
        }
        let egrid = Grid::for_bbox(b, m.max(1) as u32, med * 1.15);
        let ecells = egrid.cells();
        self.edge_grid = egrid;
        self.edge_starts.clear();
        self.edge_starts.resize(ecells + 1, 0);
        self.long_edges.clear();
        let mut eb: Vec<(u32, u32, u32, u32)> = Vec::with_capacity(m);
        for e in edges {
            let (ax, ay) = (x[e.a as usize], y[e.a as usize]);
            let (bx, by) = (x[e.b as usize], y[e.b as usize]);
            let r = Rect {
                x0: ax.min(bx),
                y0: ay.min(by),
                x1: ax.max(bx),
                y1: ay.max(by),
            };
            let (cx0, cx1, cy0, cy1) = span(&egrid, &r);
            eb.push((cx0, cx1, cy0, cy1));
            if (cx1 - cx0 + 1) as u64 * (cy1 - cy0 + 1) as u64 > 4 {
                continue;
            }
            for cy in cy0..=cy1 {
                let row = cy as usize * egrid.gw as usize;
                for cx in cx0..=cx1 {
                    self.edge_starts[row + cx as usize + 1] += 1;
                }
            }
        }
        for i in 0..ecells {
            self.edge_starts[i + 1] += self.edge_starts[i];
        }
        let mut ecur: Vec<u32> = self.edge_starts[..ecells].to_vec();
        let total = self.edge_starts[ecells] as usize;
        self.edge_items.clear();
        self.edge_items.resize(total, 0);
        for (ei, &(cx0, cx1, cy0, cy1)) in eb.iter().enumerate() {
            if (cx1 - cx0 + 1) as u64 * (cy1 - cy0 + 1) as u64 > 4 {
                self.long_edges.push(ei as u32);
                continue;
            }
            for cy in cy0..=cy1 {
                let row = cy as usize * egrid.gw as usize;
                for cx in cx0..=cx1 {
                    let c = row + cx as usize;
                    self.edge_items[ecur[c] as usize] = ei as u32;
                    ecur[c] += 1;
                }
            }
        }
    }

    /// Register a node move. Cheap: O(degree) bookkeeping. A pending full
    /// rebuild already picks up the new position, so there is nothing to do.
    pub fn mark_moved(&mut self, id: u32) {
        if self.topo_dirty || self.adj_start.len() <= id as usize + 1 {
            return;
        }
        self.dirty_nodes.push(id);
        let a = self.adj_start[id as usize] as usize;
        let b = self.adj_start[id as usize + 1] as usize;
        for k in a..b {
            self.dirty_edges.push(self.adj[k]);
        }
        if self.dirty_nodes.len() as u32 > self.dirty_limit
            || self.dirty_edges.len() as u32 > self.dirty_limit * 4
        {
            self.topo_dirty = true;
        }
    }

    /// Walk the node cells overlapping a world rect. Cells are yielded at most
    /// once; callers dedupe items (a node can sit in several cells).
    pub fn for_node_cells(&self, rect: &Rect, mut f: impl FnMut(&[u32])) {
        if self.node_items.is_empty() || !rect.is_valid() {
            return;
        }
        let g = &self.node_grid;
        let (cx0, cx1, cy0, cy1) = span(g, rect);
        for cy in cy0..=cy1 {
            let row = cy as usize * g.gw as usize;
            for cx in cx0..=cx1 {
                f(self.cell_slice(row + cx as usize));
            }
        }
    }

    /// Same for edges.
    pub fn for_edge_cells(&self, rect: &Rect, mut f: impl FnMut(&[u32])) {
        if !rect.is_valid() {
            return;
        }
        let g = &self.edge_grid;
        let (cx0, cx1, cy0, cy1) = span(g, rect);
        for cy in cy0..=cy1 {
            let row = cy as usize * g.gw as usize;
            for cx in cx0..=cx1 {
                f(self.edge_cell_slice(row + cx as usize));
            }
        }
    }

    #[inline]
    fn cell_slice(&self, c: usize) -> &[u32] {
        &self.node_items[self.node_starts[c] as usize..self.node_starts[c + 1] as usize]
    }

    #[inline]
    fn edge_cell_slice(&self, c: usize) -> &[u32] {
        &self.edge_items[self.edge_starts[c] as usize..self.edge_starts[c + 1] as usize]
    }

    #[inline]
    pub fn cell(&self, cx: u32, cy: u32) -> &[u32] {
        if self.node_starts.is_empty() {
            return &[];
        }
        let c = self.node_grid.idx(cx, cy);
        &self.node_items[self.node_starts[c] as usize..self.node_starts[c + 1] as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f32 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 33) as f32) / ((1u64 << 31) as f32)
        }
    }

    /// Grid candidates must be a superset of the brute-force answer and, after
    /// the exact test, must match it exactly.
    #[test]
    fn node_candidates_match_brute_force() {
        let mut rng = Lcg(7);
        let n = 4000usize;
        let mut x = Vec::new();
        let mut y = Vec::new();
        let mut w = Vec::new();
        let mut h = Vec::new();
        for _ in 0..n {
            x.push(rng.next() * 4000.0 - 2000.0);
            y.push(rng.next() * 4000.0 - 2000.0);
            w.push(20.0 + rng.next() * 160.0);
            h.push(20.0 + rng.next() * 60.0);
        }
        let mut idx = Index::default();
        idx.rebuild(&x, &y, &w, &h, &[]);

        let mut seen = vec![0u32; n];
        let mut out = Vec::new();
        for q in 0..64 {
            let (qx, qy) = (rng.next() * 5000.0 - 2500.0, rng.next() * 5000.0 - 2500.0);
            let (qw, qh) = (rng.next() * 900.0 + 1.0, rng.next() * 900.0 + 1.0);
            let rect = Rect {
                x0: qx,
                y0: qy,
                x1: qx + qw,
                y1: qy + qh,
            };
            out.clear();
            let gen = (q + 1) as u32;
            idx.for_node_cells(&rect, |ids| {
                for &id in ids {
                    let i = id as usize;
                    if seen[i] == gen {
                        continue;
                    }
                    seen[i] = gen;
                    let r = Rect::from_center(x[i], y[i], w[i], h[i]);
                    if r.intersects(&rect) {
                        out.push(id);
                    }
                }
            });
            out.sort_unstable();

            let mut brute: Vec<u32> = (0..n as u32)
                .filter(|&i| {
                    let r = Rect::from_center(
                        x[i as usize],
                        y[i as usize],
                        w[i as usize],
                        h[i as usize],
                    );
                    r.intersects(&rect)
                })
                .collect();
            brute.sort_unstable();
            assert_eq!(out, brute, "query {q} mismatch");
        }
    }

    #[test]
    fn moved_nodes_stay_discoverable() {
        let mut x = vec![0.0f32, 1000.0, 2000.0];
        let y = vec![0.0f32; 3];
        let w = vec![40.0f32; 3];
        let h = vec![40.0f32; 3];
        let edges = vec![Edge { a: 0, b: 1 }];
        let mut idx = Index::default();
        idx.rebuild(&x, &y, &w, &h, &edges);
        // drag node 0 far away, without rebuilding
        x[0] = 3000.0;
        idx.mark_moved(0);
        let rect = Rect {
            x0: 2900.0,
            y0: -50.0,
            x1: 3100.0,
            y1: 50.0,
        };
        let mut out = Vec::new();
        idx.for_node_cells(&rect, |ids| out.extend_from_slice(ids));
        out.extend(idx.dirty_nodes.iter().copied());
        assert!(out.contains(&0), "moved node must be a candidate");
    }
}
