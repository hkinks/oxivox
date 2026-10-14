# rustle: design spec

Superwhisper-quality dictation for Linux, written in Rust. Press a key, speak, press again: the text appears in the
app you were in. A small themed overlay with a row of voice-driven dots is the only thing on screen.

Name: **rustle** (2026-10-14). The first pick, "wisprs", clashes with the commercial speech-to-text product wisprs.co
(GitHub org `Wisprs`) and is one letter from Wispr Flow. crates.io `rustle` is an unrelated crate, so the Cargo package
is `rustle-dictation`; the app and binary are `rustle`.

Licence: MIT OR Apache-2.0.

## Goals and scope

| | |
|---|---|
| v1 scope | dictation only, done really well: hotkey, overlay, transcribe, paste |
| first target | Hyprland on NixOS, Noctalia shell, Bluetooth headset (Sony WH-1000XM5) as the usual mic |
| later targets | GNOME / KDE on Debian / Ubuntu; then Windows / macOS. The architecture keeps every desktop-specific part behind an interface |
| not in v1 | LLM modes (rewrite, translate), history UI, settings UI, voice commands for an agent, live partial text |
| offline | local model only, no network |

## Behaviour

- Hotkey: toggle (press to start, press again to stop and paste). Hold-to-talk is a later config option.
- Escape cancels while recording or transcribing (only bound then, through a Hyprland submap).
- Model: whisper large-v3-turbo (`ggml-large-v3-turbo.bin`) via whisper.cpp with the Vulkan backend, loaded once and
  kept in VRAM. Language comes from config (default: auto); the model decides what's possible.
- Model path default: `~/.local/share/rustle/models/ggml-large-v3-turbo.bin`, falling back to the voxtype copy in
  `~/.local/share/voxtype/models/`.

## Architecture

One process (daemon + overlay), a user service bound to the graphical session. A small CLI talks to it over a Unix
socket at `$XDG_RUNTIME_DIR/rustle.sock`.

Portable core (no desktop dependencies): state machine, audio buffer, `Engine` trait + whisper implementation,
voice level meter with automatic gain, overlay drawing into an RGBA pixel buffer (`tiny-skia`), config, palette.

Platform adapters, one trait each:

| trait | Hyprland / Noctalia / NixOS (v1) | GNOME / KDE (later) | Windows / macOS (later) |
|---|---|---|---|
| `Trigger` | Hyprland bind runs `rustle toggle` | GlobalShortcuts portal | native hotkey API |
| `Capture` | PipeWire (`pipewire` crate) | PipeWire | `cpal` |
| `Surface` | wlr layer-shell via `smithay-client-toolkit`, shm buffer, click-through, no keyboard focus | KDE: layer-shell; GNOME: always-on-top window or Shell extension | `winit` + `softbuffer` |
| `Output` | clipboard + Hyprland `sendshortcut` to the window focused at start | clipboard + RemoteDesktop portal | clipboard + SendInput / CGEvent |
| `Focus` | Hyprland IPC (`activewindow`) | none on GNOME Wayland | native |
| `Theme` | palette file written by a Noctalia theme template; Tokyo Night fallback | GNOME accent / KDE colour scheme | system accent |

## Record-to-paste flow

| step | daemon | overlay |
|---|---|---|
| `toggle` from idle | note focused window, open the default PipeWire source, buffer 16 kHz mono f32, pause MPRIS players | appears at once: **warming up** (dim flat line) |
| mic live | first non-zero frames (Bluetooth call-mode switch ~1 s); leading all-zero audio is dropped | dots react to the voice, in the accent colour |
| `toggle` while recording | close the stream (headset returns to music mode), run VAD energy check, then the engine | **transcribing** animation |
| result | trim; drop whisper non-speech tokens (`[BLANK_AUDIO]`, `(music)` etc.); empty → no paste | hides; empty → brief "nothing heard" flash |
| paste | save clipboard, set text, focus the step-1 window, send Ctrl+Shift+V (terminals) or Ctrl+V, restore clipboard after 500 ms, resume MPRIS | hidden |
| `cancel` | any state: discard, resume MPRIS, nothing pasted | hides |

Max recording length: config, default 300 s (stops and transcribes).

## Overlay

- Size 360 x 56 logical px, bottom centre, 48 px above the bottom edge; rounded corners (radius 12), 1 px border.
- Content: a centred row of 19 dots in the accent colour. Recording: the centre dot shows the current voice level and
  older levels ripple outwards; dots grow fast and shrink slowly; automatic gain (the loudest recent speech is a full
  dot, recovering over ~12 s), room noise stays at the smallest size. Warming up: small dim static dots. Transcribing:
  a soft pulse travels left to right. Error: text flash in the error colour. (A spectrogram was tried first and dropped
  2026-10-14: too faint at headset levels and too busy.)
- 60 fps while visible, no rendering when hidden.
- Palette: `background`, `surface`, `border`, `accent`, `text`, `error` from `$XDG_CONFIG_HOME/rustle/theme.toml`,
  reloaded on change (inotify). Noctalia writes this file through a user theme template
  (`[theme.templates.user.rustle]`). Missing file: built-in Tokyo Night.

## Errors

| case | handling |
|---|---|
| no input device | overlay error flash "no microphone"; log |
| mic never becomes live (5 s of zeros) | error flash "mic didn't start"; stop |
| model missing / GPU init fails | daemon logs and exits non-zero; systemd shows it; CLI prints the reason |
| paste target window gone | paste into whatever is focused now |
| second `toggle` while transcribing | ignored |

## Config

`$XDG_CONFIG_HOME/rustle/config.toml`: model path, language, max length, overlay size/position, paste keys per window
class. Every key optional.

## Testing

- Unit: state machine transitions, non-speech token cleanup, level meter and automatic gain on synthetic input, overlay drawing
  into a buffer (snapshot of a few states), config/theme parsing.
- Engine: one integration test on a short bundled WAV (ignored by default, needs the model + GPU).
- Manual on the target desktop: latency from stop to paste, Bluetooth warm-up, clipboard restore, terminals vs GUI apps.

## Packaging

Nix flake: package (whisper.cpp built with Vulkan), dev shell, Home Manager module (user service, config, Hyprland
binds snippet, Noctalia template). `/etc/nixos` imports it and drops voxtype once rustle covers its features.
