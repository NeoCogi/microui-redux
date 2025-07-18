#version 450
layout(location = 0) in vec2 in_tex;
layout(location = 1) in vec4 in_color;
layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D uTexture;

void main() {
    vec4 tex_color = texture(uTexture, in_tex);
    out_color = tex_color * in_color;
} 