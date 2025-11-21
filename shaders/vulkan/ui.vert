#version 450

layout(location = 0) in vec2 vertexPosition;
layout(location = 1) in vec2 vertexTexCoord;
layout(location = 2) in vec4 vertexColor;

layout(push_constant) uniform Push {
    mat4 uTransform;
} pc;

layout(location = 0) out vec2 vTexCoord;
layout(location = 1) out vec4 vVertexColor;

void main() {
    vVertexColor = vertexColor;
    vTexCoord = vertexTexCoord;
    vec4 pos = vec4(vertexPosition.x, vertexPosition.y, 0.0, 1.0);
    gl_Position = pc.uTransform * pos;
}
