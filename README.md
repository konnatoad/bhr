# Cosmic Pasta Simulator

  A Rust + WGSL black hole renderer that simulates millions of particles, bakes them into an accretion disk field, and ray-marches photons through warped spacetime.

  ## Features

  - particle-based accretion disk simulation
  - GPU ray-marched rendering with wgpu
  - PNG, TIFF, and EXR output
  - adjustable render, camera, and disk settings

  ## Usage

  PNG:
  `cargo run --release`

  TIFF:
  `cargo run --release -- --tiff`

  EXR:
  `cargo run --release -- --exr`

  All outputs:
  `cargo run --release -- --all`

  ## Notes

  - PNG uses the LDR path
  - TIFF and EXR use the HDR path
  - high resolutions use a lot of memory
