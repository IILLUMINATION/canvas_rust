//! Point picking against the indexed graph (screen coordinates in, node id out).

use crate::graph::Graph;
use crate::index::NO;
use crate::view::View;

#[inline]
fn point_in(px: f32, py: f32, x: f32, y: f32, w: f32, h: f32) -> bool {
    (px - x).abs() <= w * 0.5 && (py - y).abs() <= h * 0.5
}

/// Topmost node under the cursor (later ids paint on top), or `NO`.
/// Iterates only the single grid cell containing the point.
pub fn pick_node(g: &Graph, v: &View, sx: f32, sy: f32) -> u32 {
    if g.is_empty() {
        return NO;
    }
    let (wx, wy) = v.to_world(sx, sy);
    let gd = &g.index.node_grid;
    let cx = gd.ix(wx);
    let cy = gd.iy(wy);
    let mut best = NO;
    for &id in g.index.cell(cx, cy) {
        let i = id as usize;
        if (best == NO || id > best) && point_in(wx, wy, g.x[i], g.y[i], g.w[i], g.h[i]) {
            best = id;
        }
    }
    for i in 0..g.index.dirty_nodes.len() {
        let id = g.index.dirty_nodes[i];
        let i2 = id as usize;
        if (best == NO || id > best) && point_in(wx, wy, g.x[i2], g.y[i2], g.w[i2], g.h[i2]) {
            best = id;
        }
    }
    best
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

    #[test]
    fn pick_matches_brute_force() {
        let mut rng = Lcg(11);
        let mut g = Graph::new();
        let mut pos = Vec::new();
        for _ in 0..600 {
            let x = rng.next() * 2000.0 - 1000.0;
            let y = rng.next() * 2000.0 - 1000.0;
            pos.push((x, y));
            g.add_node(x, y, 60.0, 30.0, 0xff223344);
        }
        g.ensure_index();
        let v = View {
            cx: 0.0,
            cy: 0.0,
            scale: 0.7,
            vw: 1000.0,
            vh: 800.0,
        };
        for _ in 0..500 {
            let sx = rng.next() * 1000.0;
            let sy = rng.next() * 800.0;
            let (wx, wy) = v.to_world(sx, sy);
            let mut expect = NO;
            for (i, &(x, y)) in pos.iter().enumerate() {
                if point_in(wx, wy, x, y, 60.0, 30.0) {
                    expect = i as u32;
                }
            }
            assert_eq!(pick_node(&g, &v, sx, sy), expect);
        }
    }
}
