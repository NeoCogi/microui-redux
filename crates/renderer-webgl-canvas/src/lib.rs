//! WebGL 2 renderer for an HTML canvas.
//!
//! This backend owns the browser-specific shader contract and exposes a distinct backend/frame
//! type. Its command batching and GL resource implementation are shared with the native Glow
//! renderer.

use std::sync::Arc;

use microui_redux::{
    prelude::{AtlasHandle, AtlasUploadError, FrameError, FrameInfo, RendererBackend, RendererFrame, TextureId},
    render::{TextureError, Vertex},
};
use microui_redux_renderer_common::{CustomRenderArea, MeshSubmission};
use microui_redux_renderer_glow::{GLRenderer, GlFrame};

const VERTEX_SHADER: &str = "#version 300 es
uniform highp mat4 uTransform;
in highp vec2 vertexPosition;
in highp vec2 vertexTexCoord;
in lowp vec4 vertexColor;
out highp vec2 vTexCoord;
out lowp vec4 vVertexColor;
void main()
{
    vVertexColor = vertexColor;
    vTexCoord = vertexTexCoord;
    highp vec4 pos = vec4(vertexPosition.x, vertexPosition.y, 0.0, 1.0);
    gl_Position = uTransform * pos;
}";

const FRAGMENT_SHADER: &str = "#version 300 es
precision mediump float;
in highp vec2 vTexCoord;
in lowp vec4 vVertexColor;
uniform sampler2D uTexture;
out lowp vec4 fragmentColor;
void main()
{
    lowp vec4 col = texture(uTexture, vTexCoord);
    fragmentColor = col * vVertexColor;
}";

/// Renderer specialized for a browser WebGL 2 context attached to an HTML canvas.
pub struct WebGlCanvasRenderer {
    inner: GLRenderer,
}

impl WebGlCanvasRenderer {
    /// Creates the renderer from the canvas's current WebGL 2 context.
    pub fn new(gl: Arc<glow::Context>, atlas: AtlasHandle, width: u32, height: u32) -> Result<Self, String> {
        GLRenderer::new_with_shaders(gl, atlas, width, height, VERTEX_SHADER, FRAGMENT_SHADER).map(|inner| Self { inner })
    }
}

/// Active WebGL canvas frame.
#[must_use = "the WebGL canvas frame is finalized when dropped"]
pub struct WebGlCanvasFrame<'a> {
    inner: GlFrame<'a>,
}

impl WebGlCanvasFrame<'_> {
    /// Queues colored custom geometry inside the supplied UI area.
    pub fn enqueue_colored_vertices(&mut self, area: CustomRenderArea, vertices: Vec<Vertex>) {
        self.inner.enqueue_colored_vertices(area, vertices);
    }

    /// Queues a demo mesh draw inside the supplied UI area.
    pub fn enqueue_mesh_draw(&mut self, area: CustomRenderArea, submission: MeshSubmission) {
        self.inner.enqueue_mesh_draw(area, submission);
    }
}

impl RendererFrame for WebGlCanvasFrame<'_> {
    fn push_quad(&mut self, vertices: [Vertex; 4]) {
        self.inner.push_quad(vertices);
    }

    fn push_triangle(&mut self, vertices: [Vertex; 3]) {
        self.inner.push_triangle(vertices);
    }

    fn flush(&mut self) {
        self.inner.flush();
    }

    fn draw_texture(&mut self, id: TextureId, vertices: [Vertex; 4]) {
        self.inner.draw_texture(id, vertices);
    }
}

impl RendererBackend for WebGlCanvasRenderer {
    type Frame<'a> = WebGlCanvasFrame<'a>;

    fn get_atlas(&self) -> AtlasHandle {
        self.inner.get_atlas()
    }

    fn replace_atlas(&mut self, atlas: AtlasHandle) -> Result<(), AtlasUploadError> {
        self.inner.replace_atlas(atlas)
    }

    fn frame(&mut self, info: FrameInfo) -> Result<Self::Frame<'_>, FrameError> {
        self.inner.frame(info).map(|inner| WebGlCanvasFrame { inner })
    }

    fn create_texture(&mut self, id: TextureId, pixels: &[u8]) -> Result<(), TextureError> {
        self.inner.create_texture(id, pixels)
    }

    fn destroy_texture(&mut self, id: TextureId) {
        self.inner.destroy_texture(id);
    }
}
