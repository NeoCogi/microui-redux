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

//! Shared fixtures, renderer recordings, and no-op helpers used by unit tests.

use crate::render::{FrameError, FrameInfo, RendererBackend, RendererFrame, Vertex};
use crate::{AtlasHandle, AtlasSource, CharEntry, FontEntry, Recti, SourceFormat, TextureId, Vec2i};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::alloc::{GlobalAlloc, Layout, System};

/// Test-process allocator whose counters are enabled only inside an explicit measurement window.
pub(crate) struct CountingAllocator;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

thread_local! {
    static ALLOCATION_MEASUREMENT: Cell<Option<AllocationCount>> = const { Cell::new(None) };
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        count_allocation(pointer, layout.size());
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        count_allocation(pointer, layout.size());
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, new_size) };
        count_allocation(pointer, new_size);
        pointer
    }
}

fn count_allocation(pointer: *mut u8, bytes: usize) {
    if pointer.is_null() {
        return;
    }
    let _ = ALLOCATION_MEASUREMENT.try_with(|measurement| {
        if let Some(mut count) = measurement.get() {
            count.events = count.events.saturating_add(1);
            count.bytes = count.bytes.saturating_add(bytes as u64);
            measurement.set(Some(count));
        }
    });
}

/// Aggregate allocation and reallocation activity from one isolated test window.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct AllocationCount {
    pub(crate) events: u64,
    pub(crate) bytes: u64,
}

/// Enables allocation counters on the calling thread until [`AllocationMeasurement::finish`] is
/// called or the guard is dropped.
pub(crate) struct AllocationMeasurement {
    active: bool,
}

impl AllocationMeasurement {
    pub(crate) fn begin() -> Self {
        ALLOCATION_MEASUREMENT.with(|measurement| {
            assert!(
                measurement.replace(Some(AllocationCount::default())).is_none(),
                "allocation measurement windows must not overlap on one thread"
            );
        });
        Self { active: true }
    }

    pub(crate) fn finish(mut self) -> AllocationCount {
        self.active = false;
        ALLOCATION_MEASUREMENT.with(|measurement| measurement.replace(None).expect("allocation measurement is active"))
    }
}

impl Drop for AllocationMeasurement {
    fn drop(&mut self) {
        if self.active {
            let _ = ALLOCATION_MEASUREMENT.try_with(|measurement| measurement.set(None));
        }
    }
}

const ICON_NAMES: [&str; 9] = [
    "white",
    "close",
    "expand",
    "collapse",
    "check",
    "expand_down",
    "open_folder",
    "closed_folder",
    "file",
];

pub(crate) fn test_atlas() -> AtlasHandle {
    test_atlas_with_font_sizes(&[("default", 10)])
}

pub(crate) fn test_atlas_with_font_sizes(fonts: &[(&str, usize)]) -> AtlasHandle {
    let pixels: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
    let icons: Vec<(&str, Recti)> = ICON_NAMES.iter().map(|name| (*name, Recti::new(0, 0, 1, 1))).collect();
    let entries = vec![
        (
            '_',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(8, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        ),
        (
            'a',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(8, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        ),
        (
            'b',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(8, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        ),
    ];
    let fonts: Vec<(&str, FontEntry<'_>)> = fonts
        .iter()
        .map(|(name, size)| {
            (
                *name,
                FontEntry {
                    line_size: *size,
                    baseline: (*size as i32 * 4) / 5,
                    font_size: *size,
                    entries: &entries,
                },
            )
        })
        .collect();
    let source = AtlasSource {
        width: 1,
        height: 1,
        pixels: &pixels,
        icons: &icons,
        fonts: &fonts,
        format: SourceFormat::Raw,
    };
    AtlasHandle::from(&source)
}

pub(crate) struct NoopRenderer {
    pub(crate) atlas: AtlasHandle,
}

#[must_use]
pub(crate) struct NoopFrame;

impl RendererFrame for NoopFrame {
    fn push_quad(&mut self, _vertices: [Vertex; 4]) {}

    fn push_triangle(&mut self, _vertices: [Vertex; 3]) {}

    fn flush(&mut self) {}

    fn draw_texture(&mut self, _id: TextureId, _vertices: [Vertex; 4]) {}
}

impl RendererBackend for NoopRenderer {
    type Frame<'a> = NoopFrame;

    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn frame(&mut self, _info: FrameInfo) -> Result<Self::Frame<'_>, FrameError> {
        Ok(NoopFrame)
    }

    fn create_texture(&mut self, _id: TextureId, _width: i32, _height: i32, _pixels: &[u8]) -> Result<(), String> {
        Ok(())
    }

    fn destroy_texture(&mut self, _id: TextureId) {}
}

/// Copyable renderer-facing vertex snapshot used by characterization tests.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RecordedVertex {
    /// Screen-space vertex position.
    pub(crate) position: [f32; 2],
    /// Normalized texture coordinate.
    pub(crate) tex_coord: [f32; 2],
    /// Packed RGBA vertex color.
    pub(crate) color: [u8; 4],
}

impl From<Vertex> for RecordedVertex {
    fn from(vertex: Vertex) -> Self {
        let position = vertex.position();
        let tex_coord = vertex.tex_coord();
        let color = vertex.color();
        Self {
            position: [position.x, position.y],
            tex_coord: [tex_coord.x, tex_coord.y],
            color: [color.x, color.y, color.z, color.w],
        }
    }
}

/// Ordered renderer event captured by [`RecordingRenderer`].
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RenderEvent {
    /// A frame began with the provided dimensions and clear color.
    Begin {
        /// Viewport width.
        width: i32,
        /// Viewport height.
        height: i32,
        /// Packed RGBA clear color.
        clear: [u8; 4],
    },
    /// One atlas-backed quad was submitted.
    AtlasQuad([RecordedVertex; 4]),
    /// One triangle was submitted.
    Triangle([RecordedVertex; 3]),
    /// Buffered renderer work was flushed.
    Flush,
    /// The current frame ended.
    End,
    /// One external texture was created.
    CreateTexture {
        /// Assigned texture identifier.
        id: TextureId,
        /// Uploaded texture width.
        width: i32,
        /// Uploaded texture height.
        height: i32,
        /// Uploaded byte count.
        byte_len: usize,
    },
    /// One external texture was destroyed.
    DestroyTexture(TextureId),
    /// One external texture quad was submitted.
    ExternalTexture {
        /// Texture being drawn.
        id: TextureId,
        /// Final pre-clipped quad.
        vertices: [RecordedVertex; 4],
    },
    /// Test-owned boundary marker, including custom callbacks and slot payloads.
    Marker(String),
}

/// Cloneable ordered event log shared by renderer calls and test callbacks.
#[derive(Clone, Default)]
pub(crate) struct RenderLog {
    events: Rc<RefCell<Vec<RenderEvent>>>,
}

impl RenderLog {
    /// Returns a cloned snapshot of all events recorded so far.
    pub(crate) fn snapshot(&self) -> Vec<RenderEvent> {
        self.events.borrow().clone()
    }

    /// Clears all recorded events while retaining the log allocation.
    pub(crate) fn clear(&self) {
        self.events.borrow_mut().clear();
    }

    /// Appends a named test boundary to the event stream.
    pub(crate) fn record_marker(&self, marker: impl Into<String>) {
        self.events.borrow_mut().push(RenderEvent::Marker(marker.into()));
    }

    /// Appends one renderer event.
    fn push(&self, event: RenderEvent) {
        self.events.borrow_mut().push(event);
    }
}

/// RendererBackend implementation that records the exact backend-facing call stream.
pub(crate) struct RecordingRenderer {
    /// Atlas returned to Renderer.
    atlas: AtlasHandle,
    /// Shared event log.
    log: RenderLog,
    /// Whether texture creation should fail.
    pub(crate) fail_texture_upload: bool,
}

impl RecordingRenderer {
    /// Records a named boundary through the active typed frame.
    pub(crate) fn record_marker(&mut self, marker: impl Into<String>) {
        self.log.record_marker(marker);
    }
}

/// Creates a recording backend and its independently readable event log.
pub(crate) fn recording_backend(atlas: AtlasHandle) -> (RecordingRenderer, RenderLog) {
    let log = RenderLog::default();
    let backend = RecordingRenderer {
        atlas,
        log: log.clone(),
        fail_texture_upload: false,
    };
    (backend, log)
}

#[must_use]
pub(crate) struct RecordingFrame<'a> {
    backend: &'a mut RecordingRenderer,
}

impl RecordingFrame<'_> {
    /// Records a custom-render marker through the statically typed active frame.
    pub(crate) fn record_marker(&mut self, marker: impl Into<String>) {
        self.backend.record_marker(marker);
    }
}

impl RendererFrame for RecordingFrame<'_> {
    fn push_quad(&mut self, vertices: [Vertex; 4]) {
        self.backend.log.push(RenderEvent::AtlasQuad(vertices.map(RecordedVertex::from)));
    }

    fn push_triangle(&mut self, vertices: [Vertex; 3]) {
        self.backend.log.push(RenderEvent::Triangle(vertices.map(RecordedVertex::from)));
    }

    fn flush(&mut self) {
        self.backend.log.push(RenderEvent::Flush);
    }

    fn draw_texture(&mut self, id: TextureId, vertices: [Vertex; 4]) {
        self.backend.log.push(RenderEvent::ExternalTexture {
            id,
            vertices: vertices.map(RecordedVertex::from),
        });
    }
}

impl Drop for RecordingFrame<'_> {
    fn drop(&mut self) {
        self.backend.log.push(RenderEvent::Flush);
        self.backend.log.push(RenderEvent::End);
    }
}

impl RendererBackend for RecordingRenderer {
    type Frame<'a> = RecordingFrame<'a>;

    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn frame(&mut self, info: FrameInfo) -> Result<Self::Frame<'_>, FrameError> {
        let dimensions = info.dimensions();
        let clr = info.clear();
        self.log.push(RenderEvent::Begin {
            width: dimensions.width,
            height: dimensions.height,
            clear: [clr.r, clr.g, clr.b, clr.a],
        });
        Ok(RecordingFrame { backend: self })
    }

    fn create_texture(&mut self, id: TextureId, width: i32, height: i32, pixels: &[u8]) -> Result<(), String> {
        self.log.push(RenderEvent::CreateTexture {
            id,
            width,
            height,
            byte_len: pixels.len(),
        });
        if self.fail_texture_upload {
            Err(String::from("backend rejected texture"))
        } else {
            Ok(())
        }
    }

    fn destroy_texture(&mut self, id: TextureId) {
        self.log.push(RenderEvent::DestroyTexture(id));
    }
}
