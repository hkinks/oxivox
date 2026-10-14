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

## Project
- Push to the private GitHub repo (github.com/hkinks/rustle); make it public later.
- Remove voxtype's leftover config from `/etc/nixos/modules/dictation.nix` once rustle has proven itself.
