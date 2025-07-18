#version 450
layout(location = 0) in vec2 in_pos;
layout(location = 1) in vec2 in_tex;
layout(location = 2) in vec4 in_color;

layout(location = 0) out vec2 out_tex;
layout(location = 1) out vec4 out_color;

layout(push_constant) uniform PushConstants {
    mat4 proj;
} pc;

void main() {
    gl_Position = pc.proj * vec4(in_pos, 0.0, 1.0);
    out_tex = in_tex;
    out_color = in_color;
} 