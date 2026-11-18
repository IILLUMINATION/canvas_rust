//! Headless benchmark: graph build, index rebuild, frame assembly, picking,
//! one layout iteration. Numbers measured on the dev machine, release profile.
//!
//!   cargo run --release --bin bench

use std::time::Instant;

use canvas_core::demo;
use canvas_core::{draw::DrawList, graph::Graph, view::View, Canvas};

const VIEWPORT_W: f32 = 1200.0;
const VIEWPORT_H: f32 = 800.0;

fn median_us(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn build(n: u32) -> (Graph, View) {
    let mut g = Graph::new();
    demo::build(&mut g, n);
    g.ensure_index();
    let mut v = View {
        cx: 0.0,
        cy: 0.0,
        scale: 1.0,
        vw: VIEWPORT_W,
        vh: VIEWPORT_H,
    };
    v.fit(g.bounds(), 24.0);
    (g, v)
}

fn main() {
    println!(
        "viewport {VIEWPORT_W}x{VIEWPORT_H} logical px, release build\n\
         ---------------------------------------------------------------------------------------"
    );
    println!(
        "{:>8} {:>6} {:>9} {:>9} {:>9} {:>9} {:>10} {:>10}",
        "nodes", "edges", "build_ms", "index_ms", "frame_us", "hit_us", "vis@fit", "lod@fit"
    );

    for n in [1_000u32, 10_000, 100_000, 500_000] {
        let t = Instant::now();
        let mut g = Graph::new();
        demo::build(&mut g, n);
        let build_ms = t.elapsed().as_secs_f64() * 1e3;
        let edges = g.edges.len();

        let t = Instant::now();
        g.ensure_index();
        let index_ms = t.elapsed().as_secs_f64() * 1e3;

        let mut v = View {
            cx: 0.0,
            cy: 0.0,
            scale: 1.0,
            vw: VIEWPORT_W,
            vh: VIEWPORT_H,
        };
        v.fit(g.bounds(), 24.0);

        let mut d = DrawList::default();
        d.build(&g, &v);
        let mut samples = Vec::with_capacity(40);
        for _ in 0..40 {
            let t = Instant::now();
            d.build(&g, &v);
            samples.push(t.elapsed().as_secs_f64() * 1e6);
        }
        let frame_us = median_us(samples);
        let (vis, lod) = (d.vis_nodes, d.lod);

        // picking: 2000 points spread over the viewport
        let mut c = Canvas::new();
        c.graph = g;
        c.view = v;
        let mut hits = Vec::with_capacity(2000);
        for i in 0..2000u32 {
            let sx = (i % 40) as f32 * (VIEWPORT_W / 40.0) + 1.0;
            let sy = (i / 40) as f32 * (VIEWPORT_H / 50.0) + 1.0;
            let t = Instant::now();
            let id = c.hit(sx, sy);
            hits.push(t.elapsed().as_secs_f64() * 1e6);
            std::hint::black_box(id);
        }
        let hit_us = median_us(hits);

        println!(
            "{n:>8} {edges:>6} {build_ms:>9.1} {index_ms:>9.1} {frame_us:>9.1} {hit_us:>9.2} {vis:>10} {lod:>10}"
        );
    }

    // zoom sweep on 100k: what a phone actually pays per frame
    println!("\nzoom sweep, 100k nodes (scale = logical px per world unit)");
    let (g, _) = build(100_000);
    let mut d = DrawList::default();
    let mut v = View {
        cx: 0.0,
        cy: 0.0,
        scale: 1.0,
        vw: VIEWPORT_W,
        vh: VIEWPORT_H,
    };
    v.fit(g.bounds(), 24.0);
    let fit_scale = v.scale;
    println!(
        "{:>10} {:>8} {:>8} {:>10} {:>10}",
        "scale", "vis_nodes", "vis_edges", "frame_us", "lod"
    );
    for m in [1.0f32, 2.0, 4.0, 8.0, 16.0, 32.0] {
        v.scale = (fit_scale * m).min(8.0);
        let mut samples = Vec::new();
        for _ in 0..20 {
            let t = Instant::now();
            d.build(&g, &v);
            samples.push(t.elapsed().as_secs_f64() * 1e6);
        }
        println!(
            "{:>10.4} {:>8} {:>8} {:>10.1} {:>10}",
            v.scale,
            d.vis_nodes,
            d.vis_edges,
            median_us(samples),
            d.lod
        );
    }

    // pan sweep: 100 culled windows, worst case is a full-screen worth of nodes
    println!("\nlayout iteration cost (Barnes-Hut, theta 0.9)");
    for n in [1_000u32, 10_000, 100_000] {
        let mut c = Canvas::new();
        demo::build(&mut c.graph, n);
        let mut samples = Vec::new();
        for _ in 0..20 {
            let t = Instant::now();
            c.layout_step(1);
            samples.push(t.elapsed().as_secs_f64() * 1e3);
        }
        println!(
            "{:>8} nodes: {:>7.2} ms/iteration",
            n,
            median_us(samples) / 1000.0
        );
    }

    println!(
        "\nframe budget: 16.67 ms (60 Hz) / 33.3 ms (30 Hz). frame_us is draw-list assembly only;\n\
         GPU submission on the Flutter side is 2-3 draw calls for the whole scene."
    );
}
