//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
// this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
// this list of conditions and the following disclaimer in the documentation
// and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
// may be used to endorse or promote products derived from this software without
// specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//

//! Manual release-mode allocation, phase, and retained-structure baseline.
//!
//! Run this ignored test serially so allocation instrumentation and elapsed-time reporting are not
//! contaminated by another test thread:
//!
//! ```text
//! cargo test --release retained_runtime_baseline -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Allocation, topology, and phase counts are regression assertions. Elapsed timings are printed
//! for comparison only and deliberately have no platform-dependent pass threshold.

use crate::render::FrameInfo;
use crate::test_support::{AllocationCount, AllocationMeasurement, NoopRenderer, test_atlas};
use crate::ui_node::RuntimeMetrics;
use crate::{
    Context, Dimensioni, Key, KeyEvent, Linear, LinearParameters, Modifiers, Node, ScrollArea, ScrollAreaOption, ScrollAreaParameters, TextBlock,
    TextBlockParameters, color, rect, Window,
};
use std::hint::black_box;
use std::time::{Duration, Instant};

const ITERATIONS: u64 = 100;

fn dimensions() -> Dimensioni {
    Dimensioni::new(640, 480)
}

#[derive(Debug)]
struct OperationResult {
    allocations: AllocationCount,
    elapsed: Duration,
}

impl OperationResult {
    fn allocations_per_call(&self) -> f64 {
        self.allocations.events as f64 / ITERATIONS as f64
    }

    fn bytes_per_call(&self) -> f64 {
        self.allocations.bytes as f64 / ITERATIONS as f64
    }

    fn nanos_per_call(&self) -> u128 {
        self.elapsed.as_nanos() / u128::from(ITERATIONS)
    }
}

#[derive(Debug)]
struct ScenarioResult {
    name: &'static str,
    application_nodes: usize,
    total_nodes: usize,
    painted_nodes: u64,
    construction: AllocationCount,
    synchronization: OperationResult,
    rendering: OperationResult,
    synchronized_metrics: RuntimeMetrics,
    rendered_metrics: RuntimeMetrics,
}

fn context() -> Context<NoopRenderer> {
    Context::new_test(NoopRenderer { atlas: test_atlas() }, dimensions())
}

fn frame_info() -> FrameInfo {
    FrameInfo::try_new(dimensions(), color(0, 0, 0, 255)).expect("baseline dimensions must be positive")
}

fn measure_repeated(mut operation: impl FnMut()) -> OperationResult {
    let allocation_measurement = AllocationMeasurement::begin();
    let started = Instant::now();
    for _ in 0..ITERATIONS {
        operation();
    }
    let elapsed = started.elapsed();
    let allocations = allocation_measurement.finish();
    OperationResult { allocations, elapsed }
}

fn measure_scenario(name: &'static str, application_nodes: usize, build: impl FnOnce() -> Node) -> ScenarioResult {
    let mut ctx = context();
    let construction_measurement = AllocationMeasurement::begin();
    let content = build();
    let root = ctx.ui().create_window(Window::new(name, rect(0, 0, 600, 440), content));
    let construction = construction_measurement.finish();

    let total_nodes = ctx.debug_root_node_count(root.id()).expect("measured root must remain registered");
    assert!(
        total_nodes >= application_nodes,
        "framework-owned structural nodes can only add to the application-authored tree"
    );

    // Warm retained display-list, geometry, and traversal storage before steady measurements.
    for _ in 0..2 {
        ctx.update_ui(dimensions());
        ctx.frame(frame_info()).render_ui().expect("baseline render must succeed");
    }

    let synchronization = measure_repeated(|| ctx.update_ui(dimensions()));
    let synchronized_metrics = ctx.debug_root_runtime_metrics(root.id()).expect("measured root must expose test metrics");
    assert_eq!(synchronized_metrics.tree_layouts, 1);
    assert_eq!(synchronized_metrics.updates, 0);
    assert_eq!(synchronized_metrics.paints, 0);

    // Characterize the number of nodes eligible for paint in this viewport. Retained traversal
    // intentionally culls off-viewport nodes, so this can be smaller than the total tree size.
    ctx.update_ui(dimensions());
    ctx.frame(frame_info()).render_ui().expect("baseline render must succeed");
    let painted_nodes = ctx
        .debug_root_runtime_metrics(root.id())
        .expect("measured root must expose test metrics")
        .paints;
    assert!(painted_nodes > 0, "each visible widget body must paint at least its application root");
    assert!(
        painted_nodes <= total_nodes as u64,
        "paint traversal cannot visit more nodes than the retained tree contains"
    );

    // Start the timed rendering from another fresh commit so its metrics and measured loop contain
    // paint/submission only.
    ctx.update_ui(dimensions());
    let rendering = measure_repeated(|| {
        ctx.frame(frame_info()).render_ui().expect("baseline render must succeed");
    });
    let rendered_metrics = ctx.debug_root_runtime_metrics(root.id()).expect("measured root must expose test metrics");
    assert_eq!(rendered_metrics.tree_layouts, 1, "rendering must not add layout work");
    assert_eq!(rendered_metrics.updates, 0, "rendering must not add update work");
    assert_eq!(
        rendered_metrics.paints,
        painted_nodes * ITERATIONS,
        "every render must paint each eligible retained node once"
    );

    black_box(&ctx);
    ScenarioResult {
        name,
        application_nodes,
        total_nodes,
        painted_nodes,
        construction,
        synchronization,
        rendering,
        synchronized_metrics,
        rendered_metrics,
    }
}

fn print_scenario(result: &ScenarioResult) {
    println!(
        "| {} | {} | {} | {} | {} | {} | {} | {:.2} | {:.2} | {} | {:.2} | {:.2} | {} |",
        result.name,
        result.application_nodes,
        result.total_nodes,
        result.painted_nodes,
        result.total_nodes - result.application_nodes,
        result.construction.events,
        result.construction.bytes,
        result.synchronization.allocations_per_call(),
        result.synchronization.bytes_per_call(),
        result.synchronization.nanos_per_call(),
        result.rendering.allocations_per_call(),
        result.rendering.bytes_per_call(),
        result.rendering.nanos_per_call(),
    );
}

#[test]
#[ignore = "manual serial release-mode retained UI runtime baseline"]
fn retained_runtime_baseline() {
    let one = measure_scenario("one widget", 1, || TextBlock::create(TextBlockParameters::new("one")).1);
    let hundred = measure_scenario("100-node tree", 100, || {
        let children = (0..99).map(|index| TextBlock::create(TextBlockParameters::new(format!("node-{index}"))).1);
        Linear::create(LinearParameters::vertical(children)).1
    });
    let scroll = measure_scenario("scroll area with 20 content widgets", 22, || {
        let children = (0..20).map(|index| TextBlock::create(TextBlockParameters::new(format!("row-{index}"))).1);
        let content = Linear::create(LinearParameters::vertical(children)).1;
        ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL, content)).1
    });

    println!(
        "| scenario | application nodes | total retained nodes | painted nodes | framework structural nodes | build allocs | build bytes | sync allocs/call | sync bytes/call | sync ns/call | render allocs/call | render bytes/call | render ns/call |"
    );
    println!("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
    for result in [&one, &hundred, &scroll] {
        print_scenario(result);
        assert!(result.construction.events > 0);
        assert_eq!(result.synchronized_metrics.tree_layouts, 1);
        assert_eq!(result.rendered_metrics.tree_layouts, 1);
    }

    // Manager chrome and menu presentation are concrete surface state rather than retained widget
    // nodes. Only structural nodes intrinsic to a composite, such as ScrollArea scrollbars, add to
    // the application-authored count.
    assert_eq!((one.application_nodes, one.total_nodes), (1, 1));
    assert_eq!((hundred.application_nodes, hundred.total_nodes), (100, 100));
    assert_eq!((scroll.application_nodes, scroll.total_nodes), (22, 25));

    // Repeat the ordered transaction boundary in the same release-mode evidence run. Manager
    // chrome remains outside retained metrics; the application tree records every routed and
    // synchronization update performed for the three-event transaction.
    let mut ctx = context();
    let content = Linear::create(LinearParameters::vertical(std::iter::empty::<Node>())).1;
    let root = ctx.ui().create_window(Window::new("phase split", rect(0, 0, 120, 90), content));
    ctx.mousemove(20, 20);
    ctx.key(KeyEvent::pressed(Key::Shift, Modifiers::SHIFT));
    ctx.text("x");
    ctx.update_ui(dimensions());
    let committed = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    assert_eq!(committed.tree_layouts, 4);
    assert_eq!(committed.updates, 3);
    assert_eq!(committed.paints, 0);
    ctx.frame(frame_info()).render_ui().unwrap();
    let rendered = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    assert_eq!(rendered.tree_layouts, 4);
    assert_eq!(rendered.updates, 3);
    assert_eq!(rendered.paints, 1);
    println!("| phase | queued events | tree layouts | widget updates | paints |");
    println!("| --- | ---: | ---: | ---: | ---: |");
    println!("| update_ui | 3 | {} | {} | {} |", committed.tree_layouts, committed.updates, committed.paints);
    println!(
        "| subsequent render_ui | 0 | {} | {} | {} |",
        rendered.tree_layouts, rendered.updates, rendered.paints
    );
}
