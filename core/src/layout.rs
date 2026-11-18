//! Barnes-Hut force-directed layout.
//!
//! Repulsion is O(n log n) through a flat quadtree (radix-partitioned, no
//! per-node allocation, cells reused across iterations). Springs are O(m).
//! The caller drives it in small chunks (`step(g, iters)`) so the UI thread
//! never blocks for more than a few milliseconds.

use crate::graph::Graph;
use crate::index::NO;

const LEAF_CAP: u32 = 4;
const MAX_DEPTH: u32 = 12;
const THETA2: f32 = 0.81; // theta = 0.9

#[derive(Clone, Copy)]
struct Cell {
    mx: f32,
    my: f32,
    mass: f32,
    half: f32,
    lo: u32,
    hi: u32,
    ch: [u32; 4],
    leaf: bool,
}

impl Cell {
    const EMPTY: Cell = Cell {
        mx: 0.0,
        my: 0.0,
        mass: 0.0,
        half: 0.0,
        lo: 0,
        hi: 0,
        ch: [NO; 4],
        leaf: true,
    };
}

#[inline]
fn quadrant(x: f32, y: f32, cx: f32, cy: f32) -> usize {
    ((x >= cx) as usize) | (((y >= cy) as usize) << 1)
}

pub struct Layout {
    cells: Vec<Cell>,
    ord: Vec<u32>,
    tmp: Vec<u32>,
    stack: Vec<u32>,
    fx: Vec<f32>,
    fy: Vec<f32>,
    pub temp: f32,
    pub iter: u32,
    pub gravity: f32,
    pub spring: f32,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            cells: Vec::new(),
            ord: Vec::new(),
            tmp: Vec::new(),
            stack: Vec::new(),
            fx: Vec::new(),
            fy: Vec::new(),
            temp: 0.0,
            iter: 0,
            gravity: 0.05,
            spring: 0.5,
        }
    }
}

impl Layout {
    pub fn reset(&mut self, g: &Graph) {
        let b = g.bounds();
        let extent = if b.is_valid() {
            (b.x1 - b.x0).max(b.y1 - b.y0)
        } else {
            1000.0
        };
        self.temp = (extent * 0.1).max(8.0);
        self.iter = 0;
        self.cells.clear();
        self.ord.clear();
        self.fx.clear();
        self.fy.clear();
    }

    /// Run `iters` layout iterations in place. Positions are marked dirty and
    /// the spatial index is rebuilt once at the end.
    pub fn step(&mut self, g: &mut Graph, iters: u32) {
        let n = g.len();
        if n < 2 || iters == 0 {
            return;
        }
        if self.temp <= 0.0 || self.ord.len() != n {
            self.reset(g);
        }
        self.fx.resize(n, 0.0);
        self.fy.resize(n, 0.0);
        self.ord.resize(n, 0);
        self.tmp.resize(n, 0);
        for _ in 0..iters {
            self.iteration(g, n);
        }
        g.index.topo_dirty = true;
        g.ensure_index();
    }

    /// Advance a chunk but skip the index rebuild (interactive dragging of many
    /// nodes during an animation).
    pub fn step_light(&mut self, g: &mut Graph, iters: u32) {
        let n = g.len();
        if n < 2 || iters == 0 {
            return;
        }
        if self.temp <= 0.0 || self.ord.len() != n {
            self.reset(g);
        }
        self.fx.resize(n, 0.0);
        self.fy.resize(n, 0.0);
        self.ord.resize(n, 0);
        self.tmp.resize(n, 0);
        for _ in 0..iters {
            self.iteration(g, n);
        }
        g.index.topo_dirty = true;
    }

    fn iteration(&mut self, g: &mut Graph, n: usize) {
        let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for i in 0..n {
            let (x, y) = (g.x[i], g.y[i]);
            if x < minx {
                minx = x;
            }
            if x > maxx {
                maxx = x;
            }
            if y < miny {
                miny = y;
            }
            if y > maxy {
                maxy = y;
            }
        }
        if !(minx <= maxx && miny <= maxy) {
            minx = -1.0;
            miny = -1.0;
            maxx = 1.0;
            maxy = 1.0;
        }
        let area = ((maxx - minx) as f64 * (maxy - miny) as f64).max(1.0) as f32;
        let k = (area / n as f32).sqrt().max(64.0);
        let k2 = k * k;
        let (gx, gy) = ((minx + maxx) * 0.5, (miny + maxy) * 0.5);

        self.build_tree(&g.x, &g.y, minx, miny, maxx, maxy);

        for i in 0..n {
            let (mut fx, mut fy) = (0.0, 0.0);
            self.repulse(i as u32, g.x[i], g.y[i], k2, &g.x, &g.y, &mut fx, &mut fy);
            fx += (gx - g.x[i]) * self.gravity * k;
            fy += (gy - g.y[i]) * self.gravity * k;
            self.fx[i] = fx;
            self.fy[i] = fy;
        }

        let sf = self.spring / k;
        for e in &g.edges {
            let (a, b) = (e.a as usize, e.b as usize);
            let dx = g.x[b] - g.x[a];
            let dy = g.y[b] - g.y[a];
            let d = (dx * dx + dy * dy).sqrt().max(1e-3);
            let f = d * d * sf;
            let (ux, uy) = (dx / d, dy / d);
            self.fx[a] += ux * f;
            self.fy[a] += uy * f;
            self.fx[b] -= ux * f;
            self.fy[b] -= uy * f;
        }

        let t = self.temp;
        for i in 0..n {
            let (fx, fy) = (self.fx[i], self.fy[i]);
            let m = (fx * fx + fy * fy).sqrt();
            if m > 1e-9 {
                let s = m.min(t) / m;
                g.x[i] += fx * s;
                g.y[i] += fy * s;
            }
        }
        self.temp = (self.temp * 0.99).max(0.4);
        self.iter += 1;
    }

    fn build_tree(&mut self, xs: &[f32], ys: &[f32], minx: f32, miny: f32, maxx: f32, maxy: f32) {
        let n = xs.len();
        self.cells.clear();
        self.ord.clear();
        self.ord.extend(0..n as u32);
        let cx = (minx + maxx) * 0.5;
        let cy = (miny + maxy) * 0.5;
        let half = ((maxx - minx).max(maxy - miny) * 0.5).max(1.0) * 1.01;
        self.subdivide(0, n as u32, cx, cy, half, 0, xs, ys);
    }

    #[allow(clippy::too_many_arguments)]
    fn subdivide(
        &mut self,
        lo: u32,
        hi: u32,
        cx: f32,
        cy: f32,
        half: f32,
        depth: u32,
        xs: &[f32],
        ys: &[f32],
    ) -> u32 {
        let idx = self.cells.len() as u32;
        self.cells.push(Cell {
            half,
            lo,
            hi,
            ..Cell::EMPTY
        });
        if hi - lo <= LEAF_CAP || depth >= MAX_DEPTH {
            let mut sx = 0.0;
            let mut sy = 0.0;
            for t in lo..hi {
                let p = self.ord[t as usize] as usize;
                sx += xs[p];
                sy += ys[p];
            }
            let c = (hi - lo) as f32;
            let cell = &mut self.cells[idx as usize];
            cell.mx = sx / c;
            cell.my = sy / c;
            cell.mass = c;
            return idx;
        }

        let mut cnt = [0u32; 4];
        for t in lo..hi {
            let p = self.ord[t as usize] as usize;
            cnt[quadrant(xs[p], ys[p], cx, cy)] += 1;
        }
        let mut off = [0u32; 4];
        off[1] = cnt[0];
        off[2] = cnt[0] + cnt[1];
        off[3] = cnt[0] + cnt[1] + cnt[2];
        let mut pos = off;
        for t in lo..hi {
            let id = self.ord[t as usize];
            let p = id as usize;
            let q = quadrant(xs[p], ys[p], cx, cy);
            self.tmp[(lo + pos[q]) as usize] = id;
            pos[q] += 1;
        }
        self.ord[lo as usize..hi as usize].copy_from_slice(&self.tmp[lo as usize..hi as usize]);

        let mut mass = 0.0;
        let mut mx = 0.0;
        let mut my = 0.0;
        let qh = half * 0.5;
        for q in 0..4 {
            if cnt[q] == 0 {
                continue;
            }
            let clo = lo + off[q];
            let child = self.subdivide(
                clo,
                clo + cnt[q],
                cx + if q & 1 == 1 { qh } else { -qh },
                cy + if q & 2 == 2 { qh } else { -qh },
                qh,
                depth + 1,
                xs,
                ys,
            );
            self.cells[idx as usize].ch[q] = child;
            let cc = self.cells[child as usize];
            mass += cc.mass;
            mx += cc.mx * cc.mass;
            my += cc.my * cc.mass;
        }
        let cell = &mut self.cells[idx as usize];
        cell.leaf = false;
        cell.mass = mass;
        cell.mx = mx / mass;
        cell.my = my / mass;
        idx
    }

    #[allow(clippy::too_many_arguments)]
    fn repulse(
        &mut self,
        id: u32,
        x: f32,
        y: f32,
        k2: f32,
        xs: &[f32],
        ys: &[f32],
        fx: &mut f32,
        fy: &mut f32,
    ) {
        self.stack.clear();
        if self.cells.is_empty() {
            return;
        }
        self.stack.push(0);
        while let Some(ci) = self.stack.pop() {
            let c = self.cells[ci as usize];
            let dx = x - c.mx;
            let dy = y - c.my;
            let d2 = (dx * dx + dy * dy).max(1e-4);
            if c.leaf {
                for t in c.lo..c.hi {
                    let j = self.ord[t as usize];
                    if j == id {
                        continue;
                    }
                    let p = j as usize;
                    let ex = x - xs[p];
                    let ey = y - ys[p];
                    let e2 = (ex * ex + ey * ey).max(1e-2);
                    let inv = 1.0 / (e2 * e2.sqrt());
                    *fx += ex * k2 * inv;
                    *fy += ey * k2 * inv;
                }
            } else {
                let size = c.half * 2.0;
                if size * size < THETA2 * d2 {
                    let inv = 1.0 / (d2 * d2.sqrt());
                    *fx += dx * k2 * c.mass * inv;
                    *fy += dy * k2 * c.mass * inv;
                } else {
                    for q in 0..4 {
                        let ch = c.ch[q];
                        if ch != NO {
                            self.stack.push(ch);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;

    #[test]
    fn layout_reduces_edge_lengths() {
        let mut g = Graph::new();
        demo::build(&mut g, 500);
        let total = |g: &Graph| -> f64 {
            g.edges
                .iter()
                .map(|e| {
                    let dx = (g.x[e.a as usize] - g.x[e.b as usize]) as f64;
                    let dy = (g.y[e.a as usize] - g.y[e.b as usize]) as f64;
                    (dx * dx + dy * dy).sqrt()
                })
                .sum()
        };
        let before = total(&g);
        let mut l = Layout::default();
        l.step(&mut g, 40);
        let after = total(&g);
        assert!(after < before * 0.95, "no improvement: {before} -> {after}");
        assert!(g.x.iter().all(|v| v.is_finite()) && g.y.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn layout_keeps_graph_bounded() {
        let mut g = Graph::new();
        demo::build(&mut g, 600);
        let mut l = Layout::default();
        l.step(&mut g, 120);
        let b = g.bounds();
        assert!(b.x1 - b.x0 < 5.0e5, "graph exploded: {}", b.x1 - b.x0);
        assert!(b.y1 - b.y0 < 5.0e5, "graph exploded: {}", b.y1 - b.y0);
    }

    #[test]
    fn empty_and_single_node_are_noops() {
        let mut g = Graph::new();
        let mut l = Layout::default();
        l.step(&mut g, 10);
        g.add_node(0.0, 0.0, 10.0, 10.0, 0xff000000);
        l.step(&mut g, 10);
        assert_eq!(g.x[0], 0.0);
    }
}
