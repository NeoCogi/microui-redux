#version 450

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 inNormal;
layout(location = 2) in vec2 inUV;

layout(push_constant) uniform MeshPush {
    mat4 uPVM;
    mat4 uViewModel;
} pc;

layout(location = 0) out vec3 vNormal;
layout(location = 1) out vec3 vOrigNormal;
layout(location = 2) out vec3 vLightDir;
layout(location = 3) out vec2 vUV;

void main() {
    gl_Position = pc.uPVM * vec4(inPosition, 1.0);
    vNormal = normalize((pc.uViewModel * vec4(inNormal, 0.0)).xyz);
    vLightDir = normalize((pc.uPVM * vec4(1.0, 0.0, 0.0, 1.0))).xyz;
    vOrigNormal = inNormal;
    vUV = inUV;
}
