#[cfg(not(any(feature = "example-glow", feature = "example-vulkan")))]
compile_error!("Enable either `example-glow` or `example-vulkan` to run demo-full.");

#[cfg(feature = "example-glow")]
include!("demo_full_gl.rs");

#[cfg(feature = "example-vulkan")]
include!("demo_full_vk.rs");
