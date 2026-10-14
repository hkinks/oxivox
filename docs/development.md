# Development

Everything builds inside the Nix dev shell (Rust toolchain, cmake, libclang, Vulkan headers and loader, PipeWire,
Wayland). Keep parallelism low: whisper.cpp's Vulkan shaders are memory hungry.

```sh
nix develop
export CARGO_BUILD_JOBS=6 CMAKE_BUILD_PARALLEL_LEVEL=6
cargo build --release
cargo test --release                       # unit tests, overlay snapshots (target/overlay-snapshots/*.png)
cargo test --release -- --ignored          # engine test: needs the model and free GPU memory (RUSTLE_TEST_CPU=1 for CPU)
```

## Trying it

```sh
rustle transcribe tests/data/fox.wav       # engine check without a microphone; --cpu if the GPU is full
rustle overlay-demo                        # overlay with synthetic audio, no model, no microphone
rustle daemon                              # loads the model, shows the overlay on demand
rustle toggle                              # start recording; run again to transcribe and paste
rustle cancel                              # discard
rustle status
```

`contrib/` has a Hyprland bind snippet (including the Escape submap), a systemd user unit and example
`config.toml` / `theme.toml`.

Overlay on Hyprland (`rustle overlay-demo`): warming up, recording, transcribing, error flash.

![overlay states](overlay-states.png)

## Layout

| path | role |
|---|---|
| `src/state.rs` | pure state machine: `(State, Event) -> (State, [Action])` |
| `src/daemon.rs` | runs the machine, owns the adapters; tested with fakes in `src/daemon/tests.rs` |
| `src/platform/` | one trait per desktop concern (`mod.rs`), Hyprland / PipeWire / layer-shell implementations |
| `src/overlay/` | overlay model and tiny-skia drawing, independent of the windowing system |
| `src/engine.rs` | `Engine` trait and the whisper.cpp implementation |
| `src/spectrogram.rs`, `src/tokens.rs`, `src/audio.rs` | FFT bands, non-speech token cleanup, energy gate and WAV loading |
