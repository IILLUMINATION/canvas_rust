//! C ABI. Hand-written, dependency free, FFI-safe: no generics, no Rust types
//! crossing the boundary except pointers and PODs.
//!
//! # Safety
//!
//! Every entry point takes a raw handle from `cv_new`; the caller must not use a
//! handle after `cv_free` nor share one across threads. Buffer ownership:
//! pointers inside `CvFrame` are owned by the canvas and stay
//! valid until the next `cv_frame` / graph mutation. Dart wraps them with
//! `asTypedList` and must not keep them across frames.

#![allow(clippy::missing_safety_doc)] // the contract is documented once, above

use crate::demo;
use crate::index::NO;
use crate::Canvas;

/// Layout must match `CvFrame` in `app/lib/src/native.dart` field for field.
#[repr(C)]
pub struct CvFrame {
    pub f32_ptr: *const f32,
    pub u32_ptr: *const u32,
    pub f32_len: u32,
    pub u32_len: u32,
    pub edges_off: u32,
    pub edges_count: u32,
    pub edges_quads: u32,
    pub rects_off: u32,
    pub rects_count: u32,
    pub quads_off: u32,
    pub quads_count: u32,
    pub labels_off: u32,
    pub labels_count: u32,
    pub rect_colors_off: u32,
    pub quad_colors_off: u32,
    pub label_ids_off: u32,
    pub vis_nodes: u32,
    pub vis_edges: u32,
    pub total_nodes: u32,
    pub total_edges: u32,
    pub selected: u32,
    pub version: u32,
    pub lod: u32,
    pub build_us: u32,
}

const EMPTY: CvFrame = CvFrame {
    f32_ptr: std::ptr::null(),
    u32_ptr: std::ptr::null(),
    f32_len: 0,
    u32_len: 0,
    edges_off: 0,
    edges_count: 0,
    edges_quads: 0,
    rects_off: 0,
    rects_count: 0,
    quads_off: 0,
    quads_count: 0,
    labels_off: 0,
    labels_count: 0,
    rect_colors_off: 0,
    quad_colors_off: 0,
    label_ids_off: 0,
    vis_nodes: 0,
    vis_edges: 0,
    total_nodes: 0,
    total_edges: 0,
    selected: NO,
    version: 0,
    lod: 0,
    build_us: 0,
};

#[no_mangle]
pub extern "C" fn cv_new() -> *mut Canvas {
    Box::into_raw(Box::new(Canvas::new()))
}

#[no_mangle]
pub unsafe extern "C" fn cv_free(c: *mut Canvas) {
    if !c.is_null() {
        drop(Box::from_raw(c));
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_demo(c: *mut Canvas, n: u32, seed: u32) {
    if let Some(c) = c.as_mut() {
        demo::build_seeded(&mut c.graph, n, seed as u64 | 1);
        c.layout.reset(&c.graph);
        c.touch_structure();
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_add_node(
    c: *mut Canvas,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: u32,
) -> u32 {
    match c.as_mut() {
        Some(c) => {
            c.touch_structure();
            c.graph.add_node(x, y, w, h, color)
        }
        None => NO,
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_add_edge(c: *mut Canvas, a: u32, b: u32) -> u32 {
    match c.as_mut() {
        Some(c) => {
            c.touch_structure();
            c.graph.add_edge(a, b)
        }
        None => NO,
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_move_node(c: *mut Canvas, id: u32, x: f32, y: f32) {
    if let Some(c) = c.as_mut() {
        c.graph.set_pos(id, x, y);
        c.touch();
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_select(c: *mut Canvas, id: u32) {
    if let Some(c) = c.as_mut() {
        c.graph.select(id);
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_viewport(c: *mut Canvas, w: f32, h: f32) {
    if let Some(c) = c.as_mut() {
        c.set_viewport(w, h);
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_pan(c: *mut Canvas, dx: f32, dy: f32) {
    if let Some(c) = c.as_mut() {
        c.pan(dx, dy);
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_zoom(c: *mut Canvas, sx: f32, sy: f32, factor: f32) {
    if let Some(c) = c.as_mut() {
        c.zoom_at(sx, sy, factor);
    }
}

/// Changes whenever the scene content changes; lets the client decide whether
/// its baked overview is still valid without building a frame.
#[no_mangle]
pub unsafe extern "C" fn cv_version(c: *mut Canvas) -> u32 {
    match c.as_mut() {
        Some(c) => c.version,
        None => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_structure_version(c: *mut Canvas) -> u32 {
    match c.as_mut() {
        Some(c) => c.structure_version,
        None => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_node_w(c: *mut Canvas, id: u32) -> f32 {
    node_field(c, id, 2)
}

#[no_mangle]
pub unsafe extern "C" fn cv_node_h(c: *mut Canvas, id: u32) -> f32 {
    node_field(c, id, 3)
}

#[no_mangle]
pub unsafe extern "C" fn cv_node_color(c: *mut Canvas, id: u32) -> u32 {
    match c.as_mut() {
        Some(c) => *c.graph.color.get(id as usize).unwrap_or(&0),
        None => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_set_camera(c: *mut Canvas, cx: f32, cy: f32, scale: f32) {
    if let Some(c) = c.as_mut() {
        c.set_camera(cx, cy, scale);
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_bounds_x0(c: *mut Canvas) -> f32 {
    bnd(c, 0)
}

#[no_mangle]
pub unsafe extern "C" fn cv_bounds_y0(c: *mut Canvas) -> f32 {
    bnd(c, 1)
}

#[no_mangle]
pub unsafe extern "C" fn cv_bounds_x1(c: *mut Canvas) -> f32 {
    bnd(c, 2)
}

#[no_mangle]
pub unsafe extern "C" fn cv_bounds_y1(c: *mut Canvas) -> f32 {
    bnd(c, 3)
}

unsafe fn bnd(c: *mut Canvas, which: usize) -> f32 {
    match c.as_mut() {
        Some(c) => {
            if c.graph.is_empty() {
                return 0.0;
            }
            let b = c.graph.bounds();
            match which {
                0 => b.x0,
                1 => b.y0,
                2 => b.x1,
                _ => b.y1,
            }
        }
        None => 0.0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_zoom_centre(c: *mut Canvas, factor: f32) {
    if let Some(c) = c.as_mut() {
        c.zoom_centre(factor);
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_fit(c: *mut Canvas) {
    if let Some(c) = c.as_mut() {
        c.fit();
    }
}

/// Scalar view getters: keep the Dart side free of out-pointers and native
/// scratch memory (no allocator dependency).
#[no_mangle]
pub unsafe extern "C" fn cv_view_scale(c: *mut Canvas) -> f32 {
    match c.as_mut() {
        Some(c) => c.view.scale,
        None => 1.0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_view_cx(c: *mut Canvas) -> f32 {
    match c.as_mut() {
        Some(c) => c.view.cx,
        None => 0.0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_view_cy(c: *mut Canvas) -> f32 {
    match c.as_mut() {
        Some(c) => c.view.cy,
        None => 0.0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_node_x(c: *mut Canvas, id: u32) -> f32 {
    node_field(c, id, 0)
}

#[no_mangle]
pub unsafe extern "C" fn cv_node_y(c: *mut Canvas, id: u32) -> f32 {
    node_field(c, id, 1)
}

unsafe fn node_field(c: *mut Canvas, id: u32, field: usize) -> f32 {
    match c.as_mut() {
        Some(c) => {
            let i = id as usize;
            if i >= c.graph.len() {
                return 0.0;
            }
            match field {
                0 => c.graph.x[i],
                1 => c.graph.y[i],
                2 => c.graph.w[i],
                _ => c.graph.h[i],
            }
        }
        None => 0.0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_frame(c: *mut Canvas) -> CvFrame {
    let c = match c.as_mut() {
        Some(c) => c,
        None => return EMPTY,
    };
    c.frame();
    let d = &c.draw;
    CvFrame {
        f32_ptr: d.f.as_ptr(),
        u32_ptr: d.u.as_ptr(),
        f32_len: d.f.len() as u32,
        u32_len: d.u.len() as u32,
        edges_off: d.edges_off,
        edges_count: d.edges_n,
        edges_quads: d.edges_quads,
        rects_off: d.rects_off,
        rects_count: d.rects_n,
        quads_off: d.quads_off,
        quads_count: d.quads_n,
        labels_off: d.labels_off,
        labels_count: d.labels_n,
        rect_colors_off: d.rect_colors_off,
        quad_colors_off: d.quad_colors_off,
        label_ids_off: d.label_ids_off,
        vis_nodes: d.vis_nodes,
        vis_edges: d.vis_edges,
        total_nodes: c.graph.len() as u32,
        total_edges: c.graph.edges.len() as u32,
        selected: c.selected_visible(),
        version: c.version,
        lod: d.lod,
        build_us: (d.build_ns / 1000) as u32,
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_hit(c: *mut Canvas, sx: f32, sy: f32) -> u32 {
    match c.as_mut() {
        Some(c) => c.hit(sx, sy),
        None => NO,
    }
}

/// Runs `iters` layout iterations; returns the remaining temperature.
/// The caller stops when it reaches ~0.5.
#[no_mangle]
pub unsafe extern "C" fn cv_layout_step(c: *mut Canvas, iters: u32) -> f32 {
    match c.as_mut() {
        Some(c) => c.layout_step(iters),
        None => 0.0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn cv_layout_reset(c: *mut Canvas) {
    if let Some(c) = c.as_mut() {
        let mut l = std::mem::take(&mut c.layout);
        l.reset(&c.graph);
        c.layout = l;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Canvas;

    /// The client's rule is: a node drag may keep using a stale overview, so
    /// moves must not bump the structure version, while topology edits must.
    #[test]
    fn moves_do_not_change_structure_version() {
        unsafe {
            let c = cv_new();
            cv_demo(c, 100, 1);
            let sv = cv_structure_version(c);
            assert!(sv > 0);

            cv_move_node(c, 0, 123.0, 456.0);
            cv_move_node(c, 1, 1.0, 2.0);
            assert_eq!(cv_structure_version(c), sv, "moves must not bump structure");

            cv_add_node(c, 0.0, 0.0, 10.0, 10.0, 0xFF00FF00);
            assert_ne!(cv_structure_version(c), sv, "adding a node bumps structure");

            let sv2 = cv_structure_version(c);
            cv_add_edge(c, 0, 1);
            assert_ne!(cv_structure_version(c), sv2, "adding an edge bumps structure");

            cv_free(c);
        }
    }

    #[test]
    fn frame_carries_versions_and_geometry() {
        unsafe {
            let c = cv_new();
            cv_demo(c, 50, 7);
            cv_viewport(c, 800.0, 600.0);
            cv_fit(c);
            let f = cv_frame(c);
            assert_eq!(f.total_nodes, 50);
            assert!(f.vis_nodes > 0);
            assert!(f.build_us < 1_000_000);
            // geometry getters used by the live drag overlay
            assert_eq!(cv_node_w(c, 0), 148.0);
            assert_eq!(cv_node_h(c, 0), 50.0);
            assert_ne!(cv_node_color(c, 0), 0);
            cv_free(c);
        }
    }
}
