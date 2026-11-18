//! Frame assembly: cull -> LOD -> flat, screen-space buffers.
//!
//! Output is two flat typed arrays that Dart wraps with `asTypedList` (no copy)
//! and feeds to a handful of GPU calls: `drawRawPoints` (all edges),
//! `drawVertices` (mass node quads, one shared index buffer) or per-node
//! `drawRRect` (few nodes), plus text labels. Buffers are reused across frames
//! and written through `spare_capacity_mut`, so a steady-state frame neither
//! allocates nor zero-fills anything.
//!
//! Cost per frame is O(visible items + candidates in the touched grid cells),
//! never O(n) unless the whole graph is on screen (then `full` short-circuits
//! the index walk, the stamp arrays and the exact tests).

use std::mem::MaybeUninit;
use std::time::Instant;

use crate::graph::Graph;
use crate::index::{Rect, NO};
use crate::view::View;

pub const LOD_QUADS: u32 = 0;
pub const LOD_RECTS: u32 = 1;
pub const LOD_LABELS: u32 = 2;

/// Above this many visible nodes we drop to triangle geometry: one batched
/// `drawVertices` call instead of hundreds of `drawRRect` calls. Measured: each
/// individual rounded rect costs ~5 us of GPU submit on a weak driver, so 4000
/// of them already blow the frame budget.
pub const RECT_LIMIT: u32 = 1200;
/// Below this many visible nodes labels are worth the text layout cost.
pub const LABEL_NODE_LIMIT: u32 = 1200;
/// Hard cap on nodes whose content (title, body, badges) is emitted per frame.
/// Content is the only per-node cost that is not O(1) in triangles, so it gets
/// an explicit budget instead of scaling with the graph.
pub const LABEL_LIMIT: u32 = 256;
/// Estimated visible edges above which we stop emitting stroked segments
/// (the engine tessellates those per segment, measured at ~2 us/segment on
/// Impeller GL) and emit batched triangles instead. Conservative: the estimate
/// can undershoot in dense clusters and a wrong guess only costs quality.
pub const EDGE_LINE_LIMIT: u32 = 600;
/// Half stroke width for batch-drawn edges, screen pixels.
pub const EDGE_HALF_PX: f32 = 0.6;
/// Edges shorter than this on screen are skipped: their quads are degenerate
/// slivers that cost full rasteriser setup and contribute no pixels.
pub const EDGE_MIN_PX: f32 = 2.5;
/// A node must be at least this big on screen before its content is emitted;
/// below that the text is unreadable and only costs frame time.
pub const LABEL_MIN_W: f32 = 84.0;
pub const LABEL_MIN_H: f32 = 30.0;

const SCREEN_MARGIN: f32 = 16.0;

pub struct DrawList {
    pub f: Vec<f32>,
    pub u: Vec<u32>,

    pub edges_off: u32,
    pub edges_n: u32,
    pub edges_quads: u32,
    pub rects_off: u32,
    pub rects_n: u32,
    pub quads_off: u32,
    pub quads_n: u32,
    pub labels_off: u32,
    pub labels_n: u32,
    pub rect_colors_off: u32,
    pub quad_colors_off: u32,
    pub label_ids_off: u32,

    pub vis_nodes: u32,
    pub vis_edges: u32,
    pub lod: u32,
    pub build_ns: u64,

    vis_n: Vec<u32>,
    n_seen: Vec<u32>,
    e_seen: Vec<u32>,
    gen: u32,
}

impl Default for DrawList {
    fn default() -> Self {
        DrawList {
            f: Vec::with_capacity(1 << 16),
            u: Vec::with_capacity(1 << 12),
            edges_off: 0,
            edges_n: 0,
            edges_quads: 0,
            rects_off: 0,
            rects_n: 0,
            quads_off: 0,
            quads_n: 0,
            labels_off: 0,
            labels_n: 0,
            rect_colors_off: 0,
            quad_colors_off: 0,
            label_ids_off: 0,
            vis_nodes: 0,
            vis_edges: 0,
            lod: LOD_RECTS,
            build_ns: 0,
            vis_n: Vec::new(),
            n_seen: Vec::new(),
            e_seen: Vec::new(),
            gen: 0,
        }
    }
}

/// Reserve `cap` slots, let `fill` initialise a prefix, then commit exactly the
/// number of initialised elements. Every slot handed to `fill` is written
/// before `set_len`, so the committed region is fully initialised.
#[inline]
fn fill<T>(v: &mut Vec<T>, cap: usize, f: impl FnOnce(&mut [MaybeUninit<T>]) -> usize) -> usize {
    v.reserve(cap);
    let used = {
        let spare = v.spare_capacity_mut();
        let dst = match spare.get_mut(..cap) {
            Some(d) => d,
            None => return 0,
        };
        f(dst).min(cap)
    };
    // SAFETY: `fill` wrote `used` elements (0..=cap) into the front of the spare
    // capacity; `reserve(cap)` guarantees that capacity exists.
    unsafe { v.set_len(v.len() + used) };
    used
}

#[inline]
fn on_screen(x: f32, y: f32, w: f32, h: f32, vw: f32, vh: f32) -> bool {
    x <= vw + SCREEN_MARGIN
        && x + w >= -SCREEN_MARGIN
        && y <= vh + SCREEN_MARGIN
        && y + h >= -SCREEN_MARGIN
}

#[inline]
fn seg_on_screen(ax: f32, ay: f32, bx: f32, by: f32, vw: f32, vh: f32) -> bool {
    ax.max(bx) >= -SCREEN_MARGIN
        && ax.min(bx) <= vw + SCREEN_MARGIN
        && ay.max(by) >= -SCREEN_MARGIN
        && ay.min(by) <= vh + SCREEN_MARGIN
}

impl DrawList {
    pub fn clear(&mut self) {
        self.f.clear();
        self.u.clear();
        self.vis_n.clear();
        self.edges_n = 0;
        self.edges_quads = 0;
        self.rects_n = 0;
        self.quads_n = 0;
        self.labels_n = 0;
        self.vis_nodes = 0;
        self.vis_edges = 0;
    }

    pub fn build(&mut self, g: &Graph, v: &View) {
        let t0 = Instant::now();
        self.clear();
        let n = g.len();
        let m = g.edges.len();
        if v.vw < 1.0 || v.vh < 1.0 || v.scale <= 0.0 || n == 0 {
            self.lod = LOD_RECTS;
            self.build_ns = t0.elapsed().as_nanos() as u64;
            return;
        }
        if self.n_seen.len() < n {
            self.n_seen.resize(n, 0);
        }
        if self.e_seen.len() < m {
            self.e_seen.resize(m, 0);
        }
        self.gen = self.gen.wrapping_add(1);
        if self.gen == 0 {
            self.n_seen.fill(0);
            self.e_seen.fill(0);
            self.gen = 1;
        }
        let gen = self.gen;
        let (wx0, wy0, wx1, wy1) = v.visible_world();
        let vr = Rect {
            x0: wx0,
            y0: wy0,
            x1: wx1,
            y1: wy1,
        };

        let b = &g.index.bounds;
        let full = b.is_valid() && b.x0 >= vr.x0 && b.x1 <= vr.x1 && b.y0 >= vr.y0 && b.y1 <= vr.y1;

        let (vw, vh, sc) = (v.vw, v.vh, v.scale);
        // screen = world * sc + o
        let ox = vw * 0.5 - v.cx * sc;
        let oy = vh * 0.5 - v.cy * sc;

        // ---- density estimates (drive edge encoding and traversal mode) ----
        let (garea, varea) = if b.is_valid() {
            (
                ((b.x1 - b.x0) * (b.y1 - b.y0)).max(1.0),
                ((vr.x1 - vr.x0) * (vr.y1 - vr.y0)).max(0.0),
            )
        } else {
            (1.0, 0.0)
        };
        let area_ratio = if full { 1.0 } else { (varea / garea).min(1.0) };
        let est_edges = m as f32 * area_ratio;
        let est_nodes = n as f32 * area_ratio;
        // When most of the graph is on screen, a straight sequential scan beats
        // the index walk: no stamp array (no random 2 MB writes), and the emit
        // then reads the SoA in id order instead of hunting through memory.
        let dense = full || est_nodes * 2.0 >= n as f32;

        // ---- edges: first section ----
        // Two encodings, same single draw call either way:
        //   * stroked segments -> crisp AA lines, engine-side cost per segment;
        //   * batched triangles -> 8 floats per edge, no per-segment tessellation.
        // Which one is chosen depends on an area-based estimate of how many
        // edges are on screen, so the emit stays a single fused pass.
        let quads = est_edges > EDGE_LINE_LIMIT as f32;
        self.edges_quads = u32::from(quads);
        let stride = if quads { 8 } else { 4 };
        self.edges_off = 0;
        let e_count = {
            fill(&mut self.f, stride * m, |dst| {
                let mut cur = 0usize;
                {
                    let mut emit = |id: u32| {
                        let e = g.edges[id as usize];
                        let (ax, ay) = (g.x[e.a as usize] * sc + ox, g.y[e.a as usize] * sc + oy);
                        let (bx, by) = (g.x[e.b as usize] * sc + ox, g.y[e.b as usize] * sc + oy);
                        if !seg_on_screen(ax, ay, bx, by, vw, vh) {
                            return;
                        }
                        if (bx - ax).abs() + (by - ay).abs() < EDGE_MIN_PX {
                            return;
                        }
                        if quads {
                            let (dx, dy) = (bx - ax, by - ay);
                            let len = (dx * dx + dy * dy).sqrt();
                            if len < 1e-3 {
                                return;
                            }
                            let inv = EDGE_HALF_PX / len;
                            let nx = -dy * inv;
                            let ny = dx * inv;
                            let slot = &mut dst[cur..cur + 8];
                            slot[0].write(ax + nx);
                            slot[1].write(ay + ny);
                            slot[2].write(bx + nx);
                            slot[3].write(by + ny);
                            slot[4].write(bx - nx);
                            slot[5].write(by - ny);
                            slot[6].write(ax - nx);
                            slot[7].write(ay - ny);
                            cur += 8;
                        } else {
                            let slot = &mut dst[cur..cur + 4];
                            slot[0].write(ax);
                            slot[1].write(ay);
                            slot[2].write(bx);
                            slot[3].write(by);
                            cur += 4;
                        }
                    };
                    if dense {
                        for id in 0..m as u32 {
                            emit(id);
                        }
                    } else {
                        let seen = &mut self.e_seen;
                        g.index.for_edge_cells(&vr, |ids| {
                            for &id in ids {
                                let i = id as usize;
                                if seen[i] == gen {
                                    continue;
                                }
                                seen[i] = gen;
                                emit(id);
                            }
                        });
                        // long edges bypass the grid: one cheap bbox reject first
                        for k in 0..g.index.long_edges.len() {
                            let id = g.index.long_edges[k];
                            let j = id as usize;
                            if seen[j] == gen {
                                continue;
                            }
                            seen[j] = gen;
                            let e = g.edges[j];
                            let (ax, ay) = (g.x[e.a as usize], g.y[e.a as usize]);
                            let (bx, by) = (g.x[e.b as usize], g.y[e.b as usize]);
                            if ax.max(bx) < vr.x0
                                || ax.min(bx) > vr.x1
                                || ay.max(by) < vr.y0
                                || ay.min(by) > vr.y1
                            {
                                continue;
                            }
                            emit(id);
                        }
                        // edges incident to nodes moved since the last rebuild
                        for k in 0..g.index.dirty_edges.len() {
                            let id = g.index.dirty_edges[k];
                            let j = id as usize;
                            if j >= m || seen[j] == gen {
                                continue;
                            }
                            seen[j] = gen;
                            emit(id);
                        }
                    }
                }
                cur
            }) / stride
        };
        self.edges_n = e_count as u32;
        self.vis_edges = e_count as u32;

        // ---- visible nodes: exact cull, sequential in dense mode ----
        if full {
            self.vis_n.extend(0..n as u32);
        } else if dense {
            for i in 0..n {
                let hw = g.w[i] * sc * 0.5;
                let hh = g.h[i] * sc * 0.5;
                let x = g.x[i] * sc + ox - hw;
                let y = g.y[i] * sc + oy - hh;
                if on_screen(x, y, hw * 2.0, hh * 2.0, vw, vh) {
                    self.vis_n.push(i as u32);
                }
            }
        } else {
            {
                let seen = &mut self.n_seen;
                let vis = &mut self.vis_n;
                g.index.for_node_cells(&vr, |ids| {
                    for &id in ids {
                        let i = id as usize;
                        if seen[i] == gen {
                            continue;
                        }
                        seen[i] = gen;
                        let hw = g.w[i] * sc * 0.5;
                        let hh = g.h[i] * sc * 0.5;
                        let x = g.x[i] * sc + ox - hw;
                        let y = g.y[i] * sc + oy - hh;
                        if on_screen(x, y, hw * 2.0, hh * 2.0, vw, vh) {
                            vis.push(id);
                        }
                    }
                });
            }
            for k in 0..g.index.dirty_nodes.len() {
                let id = g.index.dirty_nodes[k];
                let i = id as usize;
                if i >= n || self.n_seen[i] == gen {
                    continue;
                }
                self.n_seen[i] = gen;
                let hw = g.w[i] * sc * 0.5;
                let hh = g.h[i] * sc * 0.5;
                let x = g.x[i] * sc + ox - hw;
                let y = g.y[i] * sc + oy - hh;
                if on_screen(x, y, hw * 2.0, hh * 2.0, vw, vh) {
                    self.vis_n.push(id);
                }
            }
        }
        let n_count = self.vis_n.len() as u32;
        self.vis_nodes = n_count;
        self.lod = if n_count <= LABEL_NODE_LIMIT {
            LOD_LABELS
        } else if n_count <= RECT_LIMIT {
            LOD_RECTS
        } else {
            LOD_QUADS
        };
        let quads = self.lod == LOD_QUADS;

        // ---- node geometry ----
        let geo_off = self.f.len() as u32;
        let col_off = self.u.len() as u32;
        if quads {
            // 4 corners + one index buffer built on the Dart side: 32 B/vertex
            // pair less per node than two explicit triangles.
            fill(&mut self.f, 8 * n_count as usize, |dst| {
                let (slots, _) = dst.as_chunks_mut::<8>();
                for (slot, &id) in slots.iter_mut().zip(self.vis_n.iter()) {
                    let i = id as usize;
                    let sx = g.x[i] * sc + ox;
                    let sy = g.y[i] * sc + oy;
                    let hw = g.w[i] * sc * 0.5;
                    let hh = g.h[i] * sc * 0.5;
                    slot[0].write(sx - hw);
                    slot[1].write(sy - hh);
                    slot[2].write(sx + hw);
                    slot[3].write(sy - hh);
                    slot[4].write(sx + hw);
                    slot[5].write(sy + hh);
                    slot[6].write(sx - hw);
                    slot[7].write(sy + hh);
                }
                8 * n_count as usize
            });
            fill(&mut self.u, 4 * n_count as usize, |dst| {
                let (cols_all, _) = dst.as_chunks_mut::<4>();
                for (cols, &id) in cols_all.iter_mut().zip(self.vis_n.iter()) {
                    let c = g.color[id as usize];
                    cols[0].write(c);
                    cols[1].write(c);
                    cols[2].write(c);
                    cols[3].write(c);
                }
                4 * n_count as usize
            });
            self.quads_off = geo_off;
            self.quad_colors_off = col_off;
            self.quads_n = n_count;
        } else {
            fill(&mut self.f, 4 * n_count as usize, |dst| {
                let (slots, _) = dst.as_chunks_mut::<4>();
                for (slot, &id) in slots.iter_mut().zip(self.vis_n.iter()) {
                    let i = id as usize;
                    let w = (g.w[i] * sc).max(1.0);
                    let h = (g.h[i] * sc).max(1.0);
                    slot[0].write(g.x[i] * sc + ox - w * 0.5);
                    slot[1].write(g.y[i] * sc + oy - h * 0.5);
                    slot[2].write(w);
                    slot[3].write(h);
                }
                4 * n_count as usize
            });
            fill(&mut self.u, n_count as usize, |dst| {
                for (col, &id) in dst.iter_mut().zip(self.vis_n.iter()) {
                    col.write(g.color[id as usize]);
                }
                n_count as usize
            });
            self.rects_off = geo_off;
            self.rect_colors_off = col_off;
            self.rects_n = n_count;
        }

        // ---- labels: rect + id, capped ----
        self.f.reserve(4 * LABEL_LIMIT as usize);
        self.u.reserve(LABEL_LIMIT as usize);
        self.labels_off = self.f.len() as u32;
        self.label_ids_off = self.u.len() as u32;
        if !quads {
            let mut emitted = 0u32;
            for k in 0..self.vis_n.len() {
                if emitted >= LABEL_LIMIT {
                    break;
                }
                let i = self.vis_n[k] as usize;
                let w = g.w[i] * sc;
                let h = g.h[i] * sc;
                if w < LABEL_MIN_W || h < LABEL_MIN_H {
                    continue;
                }
                self.f.push(g.x[i] * sc + ox - w * 0.5);
                self.f.push(g.y[i] * sc + oy - h * 0.5);
                self.f.push(w);
                self.f.push(h);
                self.u.push(self.vis_n[k]);
                emitted += 1;
            }
            self.labels_n = emitted;
        }

        self.build_ns = t0.elapsed().as_nanos() as u64;
    }

    /// Selected node id if it is in the current frame, else `NO`.
    pub fn selected_visible(&self, g: &Graph) -> u32 {
        if g.selected == NO {
            return NO;
        }
        match self.n_seen.get(g.selected as usize) {
            Some(&s) if s == self.gen => g.selected,
            _ => NO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;

    fn view_for(g: &Graph, scale: f32) -> View {
        let b = g.bounds();
        View {
            cx: (b.x0 + b.x1) * 0.5,
            cy: (b.y0 + b.y1) * 0.5,
            scale,
            vw: 1200.0,
            vh: 800.0,
        }
    }

    fn fit_view(g: &Graph) -> View {
        let mut v = View {
            cx: 0.0,
            cy: 0.0,
            scale: 1.0,
            vw: 1200.0,
            vh: 800.0,
        };
        v.fit(g.bounds(), 0.0);
        v
    }

    #[test]
    fn sections_tile_both_buffers_exactly() {
        let mut g = Graph::new();
        demo::build(&mut g, 3000);
        g.ensure_index();
        let v = view_for(&g, 0.3);
        let mut d = DrawList::default();
        d.build(&g, &v);
        assert_eq!(d.edges_off, 0);
        let nodes_off =
            d.edges_off as usize + (if d.edges_quads == 1 { 8 } else { 4 }) * d.edges_n as usize;
        let nodes_len = if d.quads_n > 0 {
            assert_eq!(d.rects_n, 0);
            assert_eq!(d.quads_off as usize, nodes_off);
            8 * d.quads_n as usize
        } else {
            assert_eq!(d.quads_n, 0);
            assert_eq!(d.rects_off as usize, nodes_off);
            4 * d.rects_n as usize
        };
        assert_eq!(d.labels_off as usize, nodes_off + nodes_len);
        assert_eq!(d.labels_off as usize + 4 * d.labels_n as usize, d.f.len());
        assert_eq!(
            d.vis_nodes,
            if d.quads_n > 0 { d.quads_n } else { d.rects_n }
        );
        assert_eq!(d.vis_edges, d.edges_n);
        assert!(d.edges_n > 0 && d.vis_nodes > 0);
        let colors = if d.quads_n > 0 {
            4 * d.quads_n
        } else {
            d.rects_n
        };
        assert_eq!(d.u.len() as u32, colors + d.labels_n);
        assert!(d.u.iter().all(|c| (c >> 24) != 0));
        assert!(d.f.iter().all(|c| c.is_finite()));
    }

    /// Culled counts must equal a brute-force scan for the same viewport.
    #[test]
    fn culling_matches_brute_force() {
        let mut g = Graph::new();
        demo::build(&mut g, 800);
        g.ensure_index();
        for scale in [0.05f32, 0.2, 0.7, 2.0] {
            let v = view_for(&g, scale);
            let mut d = DrawList::default();
            d.build(&g, &v);
            let (ox, oy) = (v.vw * 0.5 - v.cx * scale, v.vh * 0.5 - v.cy * scale);
            let mut brute_n = 0u32;
            for i in 0..g.len() {
                let sx = g.x[i] * scale + ox;
                let sy = g.y[i] * scale + oy;
                let hw = g.w[i] * scale * 0.5;
                let hh = g.h[i] * scale * 0.5;
                if on_screen(sx - hw, sy - hh, hw * 2.0, hh * 2.0, v.vw, v.vh) {
                    brute_n += 1;
                }
            }
            assert_eq!(d.vis_nodes, brute_n, "nodes @ scale {scale}");

            let mut brute_e = 0u32;
            for e in g.edges.iter() {
                let ax = g.x[e.a as usize] * scale + ox;
                let ay = g.y[e.a as usize] * scale + oy;
                let bx = g.x[e.b as usize] * scale + ox;
                let by = g.y[e.b as usize] * scale + oy;
                if seg_on_screen(ax, ay, bx, by, v.vw, v.vh) {
                    brute_e += 1;
                }
            }
            assert_eq!(d.vis_edges, brute_e, "edges @ scale {scale}");
        }
    }

    #[test]
    fn dragged_node_is_always_emitted() {
        let mut g = Graph::new();
        demo::build(&mut g, 800);
        g.ensure_index();
        let v = view_for(&g, 0.5);
        let (wx, wy) = v.to_world(v.vw * 0.5, v.vh * 0.5);
        let id = 5u32;
        g.set_pos(id, wx, wy);
        let mut d = DrawList::default();
        d.build(&g, &v);
        // the moved node must be present in the emitted geometry
        let mut found = false;
        if d.quads_n > 0 {
            let (quads, _) = d.f[d.quads_off as usize..].as_chunks::<8>();
            for q in quads.iter() {
                let cx = (q[0] + q[2]) * 0.5;
                let cy = (q[1] + q[5]) * 0.5;
                if (cx - v.vw * 0.5).abs() < 2.0 && (cy - v.vh * 0.5).abs() < 2.0 {
                    found = true;
                    break;
                }
            }
        } else {
            let (rects, _) = d.f[d.rects_off as usize..d.labels_off as usize].as_chunks::<4>();
            for r in rects.iter() {
                if (r[0] + r[2] * 0.5 - v.vw * 0.5).abs() < 2.0
                    && (r[1] + r[3] * 0.5 - v.vh * 0.5).abs() < 2.0
                {
                    found = true;
                    break;
                }
            }
        }
        assert!(found, "dragged node must be emitted at the viewport centre");
    }

    #[test]
    fn emits_quads_for_dense_views() {
        let mut g = Graph::new();
        demo::build(&mut g, 40000);
        g.ensure_index();
        let v = fit_view(&g);
        let mut d = DrawList::default();
        d.build(&g, &v);
        assert!(
            d.vis_nodes > RECT_LIMIT,
            "expected dense frame, got {}",
            d.vis_nodes
        );
        assert_eq!(d.lod, LOD_QUADS);
        assert_eq!(d.quads_n, d.vis_nodes);
        assert_eq!(d.rects_n, 0);
    }

    #[test]
    fn buffers_do_not_grow_after_warmup() {
        let mut g = Graph::new();
        demo::build(&mut g, 5000);
        g.ensure_index();
        let v = view_for(&g, 0.4);
        let mut d = DrawList::default();
        d.build(&g, &v);
        let (fc, uc) = (d.f.capacity(), d.u.capacity());
        for _ in 0..5 {
            d.build(&g, &v);
        }
        assert_eq!((fc, uc), (d.f.capacity(), d.u.capacity()));
    }
}
