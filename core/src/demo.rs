//! Deterministic demo graph so the engine can be exercised without data import.

use crate::graph::Graph;

pub const PALETTE: [u32; 6] = [
    0xFF4F8CFF, 0xFF33C6A3, 0xFFFFB020, 0xFFE0524E, 0xFF9B6BFF, 0xFF4FC3F7,
];

pub const NODE_W: f32 = 148.0;
pub const NODE_H: f32 = 50.0;

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }
    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// Uniform in [0, 1).
    #[inline]
    fn f(&mut self) -> f32 {
        ((self.next_u64() >> 40) as f32) / ((1u64 << 24) as f32)
    }
    #[inline]
    fn range(&mut self, n: u32) -> u32 {
        (self.next_u64() % n.max(1) as u64) as u32
    }
}

/// Cluster-structured graph: `n` nodes in nested rings, ring edges plus random
/// chords. Roughly 1.5 edges per node.
pub fn build(g: &mut Graph, n: u32) {
    build_seeded(g, n, 0x9E37_79B9_7F4A_7C15);
}

pub fn build_seeded(g: &mut Graph, n: u32, seed: u64) {
    g.clear();
    if n == 0 {
        return;
    }
    g.reserve(n as usize, (n as f64 * 1.6) as usize);
    let mut rng = Rng::new(seed);

    let target_cluster = ((n as f32).sqrt() as u32).clamp(1, 512);
    let per = (n / target_cluster).max(1);
    let clusters = n.div_ceil(per).max(1);
    let cols = (clusters as f32).sqrt().ceil().max(1.0) as u32;
    let spacing = 2400.0f32;

    for i in 0..n {
        let c = i / per;
        let k = i % per;
        let cx = (c % cols) as f32 - (cols as f32 - 1.0) * 0.5;
        let cy = (c / cols) as f32 - (clusters.div_ceil(cols) as f32 - 1.0) * 0.5;
        // golden-angle spiral keeps nodes apart without a layout pass
        let ang = k as f32 * 2.399_963_2 + c as f32 * 0.7;
        let rad = (k as f32).sqrt() * 150.0;
        let x = cx * spacing + ang.cos() * rad + (rng.f() - 0.5) * 24.0;
        let y = cy * spacing + ang.sin() * rad + (rng.f() - 0.5) * 24.0;
        g.add_node(x, y, NODE_W, NODE_H, PALETTE[c as usize % PALETTE.len()]);
    }

    // ring edges inside each cluster
    for c in 0..clusters {
        let start = c * per;
        let end = (start + per).min(n);
        if end - start < 2 {
            continue;
        }
        for i in start..end - 1 {
            g.add_edge(i, i + 1);
        }
        g.add_edge(end - 1, start);
    }
    // chords, mostly local, occasionally cross-cluster
    let chords = (n as f32 * 0.45) as u32;
    for _ in 0..chords {
        let a = rng.range(n);
        let b = if rng.f() < 0.15 {
            rng.range(n)
        } else {
            (a + rng.range(per.max(2))) % n
        };
        g.add_edge(a, b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let mut a = Graph::new();
        let mut b = Graph::new();
        build_seeded(&mut a, 500, 42);
        build_seeded(&mut b, 500, 42);
        assert_eq!(a.x, b.x);
        assert_eq!(a.edges.len(), b.edges.len());
        assert_eq!(a.len(), 500);
        assert!(!a.edges.is_empty());
    }

    #[test]
    fn no_degenerate_edges() {
        let mut g = Graph::new();
        build(&mut g, 2000);
        assert!(g
            .edges
            .iter()
            .all(|e| e.a != e.b && e.a < 2000 && e.b < 2000));
    }
}
