# Backlog

Ideas and follow-ups not in the current milestone. Newest at the bottom of each section.

## Engine
- Quantized model: whisper.cpp q8_0 / q5_0 variants of large-v3-turbo (1.6 GB f16 today) to cut VRAM use; measure
  accuracy and speed against f16 before switching the default.
- Other engines behind the `Engine` trait (e.g. NVIDIA FastConformer / Parakeet via ONNX).
- CUDA backend as an alternative to Vulkan.

## Behaviour
- Hold-to-talk as a config option next to toggle.

## Overlay
- Check fractional and integer scales above 1.0 on a real compositor (only covered by snapshot tests).
- Neutral (warming up) and accent (listening) dots can look alike with desaturated themes (e.g. Noctalia's current
  `#8baec1` accent); consider a stronger difference than colour alone.

## Platforms
- GNOME / KDE adapters (GlobalShortcuts and RemoteDesktop portals, overlay surface on GNOME).
- Windows / macOS.

## Robustness
- Signal handling: SIGTERM or a crash during a recording leaves Hyprland in the cancel submap and media paused.
- Capture events carry no session id; samples from a just-stopped stream can leak into an immediately following one.
- A failed paste leaves the transcript in the clipboard without restoring the saved one (arguably useful; document or
  restore).
- `wl-paste` is called without a timeout; an unresponsive clipboard owner blocks the daemon thread.
- Without `XDG_RUNTIME_DIR` the socket falls back to the shared temp dir; use a per-user directory or refuse.
- Unset `HOME` makes config and data paths relative; `max_length_secs = 0` is not validated.
- The overlay thread dying is silent; log it.
