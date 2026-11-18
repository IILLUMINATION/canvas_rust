//! Camera transform. World units -> logical screen pixels.
//!
//! `cx/cy` is the world point shown at the viewport centre, `scale` is logical
//! pixels per world unit. All culling math runs in world space; the draw list is
//! emitted already transformed so the Flutter side never touches matrices.

use crate::index::Rect;

pub const MIN_SCALE: f32 = 0.02;
pub const MAX_SCALE: f32 = 8.0;

#[derive(Clone, Copy, Debug)]
pub struct View {
    pub cx: f32,
    pub cy: f32,
    pub scale: f32,
    pub vw: f32,
    pub vh: f32,
}

impl Default for View {
    fn default() -> Self {
        View {
            cx: 0.0,
            cy: 0.0,
            scale: 1.0,
            vw: 1.0,
            vh: 1.0,
        }
    }
}

impl View {
    #[inline]
    pub fn to_screen(&self, x: f32, y: f32) -> (f32, f32) {
        (
            (x - self.cx) * self.scale + self.vw * 0.5,
            (y - self.cy) * self.scale + self.vh * 0.5,
        )
    }

    #[inline]
    pub fn to_world(&self, sx: f32, sy: f32) -> (f32, f32) {
        (
            self.cx + (sx - self.vw * 0.5) / self.scale,
            self.cy + (sy - self.vh * 0.5) / self.scale,
        )
    }

    /// Visible world rect, expanded by a screen-space margin so that strokes and
    /// node borders that straddle the edge are still emitted.
    #[inline]
    pub fn visible_world(&self) -> (f32, f32, f32, f32) {
        let m = 24.0 / self.scale;
        let hw = self.vw * 0.5 / self.scale + m;
        let hh = self.vh * 0.5 / self.scale + m;
        (self.cx - hw, self.cy - hh, self.cx + hw, self.cy + hh)
    }

    pub fn pan_px(&mut self, dx: f32, dy: f32) {
        self.cx -= dx / self.scale;
        self.cy -= dy / self.scale;
    }

    /// Zoom keeping the world point under (sx, sy) pinned to the cursor.
    pub fn zoom_at(&mut self, sx: f32, sy: f32, factor: f32) {
        let (wx, wy) = self.to_world(sx, sy);
        let s = (self.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        if s == self.scale {
            return;
        }
        self.scale = s;
        self.cx = wx - (sx - self.vw * 0.5) / s;
        self.cy = wy - (sy - self.vh * 0.5) / s;
    }

    /// Zoom around the viewport centre (keyboard/buttons).
    pub fn zoom_centre(&mut self, factor: f32) {
        let (cx, cy) = (self.vw * 0.5, self.vh * 0.5);
        self.zoom_at(cx, cy, factor);
    }

    pub fn fit(&mut self, bounds: Rect, pad_px: f32) {
        if !bounds.is_valid() {
            return;
        }
        let w = (bounds.x1 - bounds.x0).max(1.0);
        let h = (bounds.y1 - bounds.y0).max(1.0);
        let sx = ((self.vw - 2.0 * pad_px) / w).max(0.001);
        let sy = ((self.vh - 2.0 * pad_px) / h).max(0.001);
        self.scale = sx.min(sy).clamp(MIN_SCALE, MAX_SCALE);
        self.cx = (bounds.x0 + bounds.x1) * 0.5;
        self.cy = (bounds.y0 + bounds.y1) * 0.5;
    }
}
