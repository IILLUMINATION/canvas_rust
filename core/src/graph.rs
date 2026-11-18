//! Structure-of-arrays graph. No per-node allocation, ids are stable indices.

use crate::index::{Index, Rect, NO};

#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub a: u32,
    pub b: u32,
}

#[derive(Default)]
pub struct Graph {
    pub x: Vec<f32>,
    pub y: Vec<f32>,
    pub w: Vec<f32>,
    pub h: Vec<f32>,
    pub color: Vec<u32>,
    pub edges: Vec<Edge>,
    pub selected: u32,
    pub index: Index,
}

impl Graph {
    pub fn new() -> Self {
        Graph {
            selected: NO,
            ..Default::default()
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.x.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.x.is_empty()
    }

    pub fn clear(&mut self) {
        self.x.clear();
        self.y.clear();
        self.w.clear();
        self.h.clear();
        self.color.clear();
        self.edges.clear();
        self.selected = NO;
        self.index = Index::default();
    }

    pub fn reserve(&mut self, nodes: usize, edges: usize) {
        self.x.reserve(nodes);
        self.y.reserve(nodes);
        self.w.reserve(nodes);
        self.h.reserve(nodes);
        self.color.reserve(nodes);
        self.edges.reserve(edges);
    }

    pub fn add_node(&mut self, x: f32, y: f32, w: f32, h: f32, color: u32) -> u32 {
        let id = self.x.len() as u32;
        self.x.push(x);
        self.y.push(y);
        self.w.push(w);
        self.h.push(h);
        self.color.push(color);
        self.index.topo_dirty = true;
        id
    }

    /// Returns the new edge id, or `NO` for out-of-range / self loops.
    pub fn add_edge(&mut self, a: u32, b: u32) -> u32 {
        let n = self.x.len() as u32;
        if a >= n || b >= n || a == b {
            return NO;
        }
        let id = self.edges.len() as u32;
        self.edges.push(Edge { a, b });
        self.index.topo_dirty = true;
        id
    }

    #[inline]
    pub fn set_pos(&mut self, id: u32, x: f32, y: f32) {
        let i = id as usize;
        if i < self.x.len() {
            self.x[i] = x;
            self.y[i] = y;
            self.index.mark_moved(id);
        }
    }

    pub fn set_size(&mut self, id: u32, w: f32, h: f32) {
        let i = id as usize;
        if i < self.x.len() {
            self.w[i] = w;
            self.h[i] = h;
            self.index.topo_dirty = true;
        }
    }

    pub fn select(&mut self, id: u32) {
        self.selected = if (id as usize) < self.x.len() { id } else { NO };
    }

    /// Rebuild the spatial index if it is stale. Amortised, not per frame.
    pub fn ensure_index(&mut self) {
        if self.index.topo_dirty {
            self.index
                .rebuild(&self.x, &self.y, &self.w, &self.h, &self.edges);
        }
    }

    pub fn node_rect(&self, id: u32) -> Rect {
        let i = id as usize;
        if i < self.x.len() {
            Rect::from_center(self.x[i], self.y[i], self.w[i], self.h[i])
        } else {
            Rect::EMPTY
        }
    }

    pub fn bounds(&self) -> Rect {
        let mut b = Rect::EMPTY;
        for i in 0..self.x.len() {
            b.extend(self.x[i] - self.w[i] * 0.5, self.y[i] - self.h[i] * 0.5);
            b.extend(self.x[i] + self.w[i] * 0.5, self.y[i] + self.h[i] * 0.5);
        }
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_edges() {
        let mut g = Graph::new();
        g.add_node(0.0, 0.0, 10.0, 10.0, 0xff000000);
        assert_eq!(g.add_edge(0, 0), NO);
        assert_eq!(g.add_edge(0, 5), NO);
        assert_eq!(g.add_edge(0, 0), NO);
        g.add_node(5.0, 5.0, 10.0, 10.0, 0xff000000);
        assert_eq!(g.add_edge(0, 1), 0);
    }

    #[test]
    fn selection_clears_on_invalid_id() {
        let mut g = Graph::new();
        g.add_node(0.0, 0.0, 10.0, 10.0, 0xff000000);
        g.select(0);
        assert_eq!(g.selected, 0);
        g.select(99);
        assert_eq!(g.selected, NO);
    }
}
