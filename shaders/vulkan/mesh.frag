#version 450

layout(location = 0) in vec3 vNormal;
layout(location = 1) in vec3 vOrigNormal;
layout(location = 2) in vec3 vLightDir;
layout(location = 3) in vec2 vUV;

layout(location = 0) out vec4 outColor;

void main() {
    vec3 n = normalize(vNormal);
    vec3 l = normalize(vLightDir);
    float intensity = max(dot(l, n), 0.0);
    vec3 base = vOrigNormal * 0.9 + vec3(vUV, 0.0) * 0.1;
    vec3 finalColor = base * intensity + vec3(0.1);
    outColor = vec4(finalColor, 1.0);
}
