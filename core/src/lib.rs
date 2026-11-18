//! Graph canvas engine core.
//!
//! The Rust side owns the graph, the camera, spatial indexing, culling, level of
//! detail selection and picking. It emits one flat, screen-space draw list per
//! frame that the Flutter side uploads with a handful of GPU calls.

pub mod demo;
pub mod draw;
pub mod ffi;
pub mod graph;
pub mod hit;
pub mod index;
pub mod layout;
pub mod view;

use draw::DrawList;

use graph::Graph;
use index::NO;
use layout::Layout;
use view::View;

pub const FIT_PADDING_PX: f32 = 48.0;

pub struct Canvas {
    pub graph: Graph,
    pub view: View,
    pub draw: DrawList,
    pub layout: Layout,
    /// Bumped on every mutation that changes what a frame looks like.
    pub version: u32,
    /// Bumped only by topology/size changes. Node moves leave it alone, so the
    /// client can keep panning a slightly stale overview bitmap while a node is
    /// being dragged instead of falling back to drawing 300k quads per frame.
    pub structure_version: u32,
}

impl Default for Canvas {
    fn default() -> Self {
        Canvas::new()
    }
}

impl Canvas {
    pub fn new() -> Self {
        Canvas {
            graph: Graph::new(),
            view: View {
                cx: 0.0,
                cy: 0.0,
                scale: 1.0,
                vw: 1.0,
                vh: 1.0,
            },
            draw: DrawList::default(),
            layout: Layout::default(),
            version: 1,
            structure_version: 1,
        }
    }

    /// Rebuild the index if stale, then produce the draw list for this frame.
    pub fn frame(&mut self) {
        self.graph.ensure_index();
        self.draw.build(&self.graph, &self.view);
    }

    pub fn set_viewport(&mut self, w: f32, h: f32) {
        self.view.vw = w.max(1.0);
        self.view.vh = h.max(1.0);
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.view.pan_px(dx, dy);
    }

    pub fn zoom_at(&mut self, sx: f32, sy: f32, factor: f32) {
        self.view.zoom_at(sx, sy, factor);
    }

    #[inline]
    pub fn touch(&mut self) {
        self.version = self.version.wrapping_add(1);
    }

    #[inline]
    pub fn touch_structure(&mut self) {
        self.version = self.version.wrapping_add(1);
        self.structure_version = self.structure_version.wrapping_add(1);
    }

    pub fn set_camera(&mut self, cx: f32, cy: f32, scale: f32) {
        self.view.cx = cx;
        self.view.cy = cy;
        self.view.scale = scale.clamp(crate::view::MIN_SCALE, crate::view::MAX_SCALE);
    }

    pub fn zoom_centre(&mut self, factor: f32) {
        self.view.zoom_centre(factor);
    }

    pub fn fit(&mut self) {
        self.view.fit(self.graph.bounds(), FIT_PADDING_PX);
    }

    pub fn hit(&mut self, sx: f32, sy: f32) -> u32 {
        self.graph.ensure_index();
        hit::pick_node(&self.graph, &self.view, sx, sy)
    }

    pub fn select(&mut self, id: u32) {
        self.graph.select(id);
    }

    pub fn layout_step(&mut self, iters: u32) -> f32 {
        self.layout.step(&mut self.graph, iters);
        self.touch();
        self.layout.temp
    }

    pub fn node_count(&self) -> u32 {
        self.graph.len() as u32
    }

    pub fn edge_count(&self) -> u32 {
        self.graph.edges.len() as u32
    }

    pub fn selected_visible(&self) -> u32 {
        if self.graph.selected == NO {
            return NO;
        }
        self.draw.selected_visible(&self.graph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_smoke() {
        let mut c = Canvas::new();
        c.set_viewport(1000.0, 700.0);
        demo::build(&mut c.graph, 2000);
        c.fit();
        c.frame();
        let d = &c.draw;
        assert!(d.vis_nodes > 0 && d.vis_edges > 0);
        assert!(d.vis_nodes <= 2000);
        assert!(d.build_ns > 0);
        // picking the centre of the viewport, when it hits, must be a node
        let id = c.hit(500.0, 350.0);
        assert!(id == NO || (id as usize) < c.graph.len());
    }

    #[test]
    fn empty_frame_is_safe() {
        let mut c = Canvas::new();
        c.set_viewport(400.0, 300.0);
        c.frame();
        assert_eq!(c.draw.vis_nodes, 0);
        assert_eq!(c.draw.edges_n, 0);
        assert_eq!(c.hit(10.0, 10.0), NO);
    }
}
