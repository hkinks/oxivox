# Landscape: local push-to-talk dictation (2026-10-15)

Where oxivox sits among open-source (and a couple of commercial) dictation tools. Stars and last-push dates come from the GitHub API on 2026-10-15. Feature columns come from each project's README and docs, so "not mentioned" means the README does not say, not that the feature is absent. Superwhisper and Wispr Flow rows are from general knowledge and were not re-verified.

## Comparison

| Project | Platforms | Stack | Engines | GPU | Wayland / Hyprland | Paste method | Overlay / UI | LLM post-proc. | Nix | Stars, last push, license |
|---|---|---|---|---|---|---|---|---|---|---|
| **oxivox** | Linux (Hyprland first) | Rust daemon | whisper.cpp large-v3-turbo | Vulkan, CPU fallback | Hyprland only (IPC) | Hyprland `sendshortcut` to the window focused at record start; Ctrl+Shift+V for terminals; clipboard restored | Layer-shell dots, palette-themed with live reload | none | flake + Home Manager | new, 2026-10-15, MIT or Apache-2.0 |
| [Handy](https://github.com/cjpais/Handy) | Linux, macOS, Windows | Rust + Tauri | whisper.cpp (Small to Large), Parakeet V3; Silero VAD | yes, where available | "Limited": needs wtype or dotool | wtype/dotool/enigo typing, or paste | small recording overlay; settings GUI | optional post-processing toggle | flake in repo | 33.8k, 2026-10-15, MIT |
| [OpenWhispr](https://github.com/OpenWhispr/openwhispr) | Linux, macOS, Windows | Electron/JS | whisper.cpp, Parakeet, Cohere, cloud | CUDA, Metal, Vulkan | works; details not in README | auto-paste at cursor | full app: notes, meetings, history | yes (cloud or local llama.cpp), voice-assistant hotkey | flake in repo | 9.4k, 2026-10-15, MIT |
| [Voxtype](https://github.com/peteonrails/voxtype) | Linux (macOS via Homebrew) | Rust | 9 engines: Whisper, Parakeet, Cohere Transcribe, Moonshine, SenseVoice, OpenVINO and others | CUDA, ROCm/MIGraphX, Vulkan, Intel NPU | Hyprland, Niri, Sway, River, GNOME, KDE | wtype, then dotool, ydotool, clipboard, or Ctrl+V | floating waveform OSD, TUI configure | optional pipe through any LLM or script | flake in repo | 1.6k, 2026-10-15, MIT |
| [hyprwhspr](https://github.com/goodroot/hyprwhspr) | Linux | Python (onnx-asr, pywhispercpp) | Whisper, Parakeet, Cohere, Qwen3-ASR, cloud REST | CUDA, Vulkan | Wayland and X11; Hyprland-centred | auto-paste via wl-clipboard + wtype | GTK4 visualizer, auto-matches Omarchy and Noctalia themes | word overrides, prompts; no LLM cleanup noted | flake + NixOS module (beta) | 1.2k, 2026-10-15, MIT |
| [Vocalinux](https://github.com/VocaHQ/vocalinux) | Linux | Python | whisper.cpp default, Faster Whisper, Vosk, Parakeet, HTTP API | Vulkan | X11 + Wayland | xdotool, IBus, wtype, ydotool, clipboard | audio-reactive orb overlay, tray | none noted | Nix packaging | 939, 2026-10-15, AGPL-3.0 |
| [whisrs](https://github.com/y0sif/whisrs) | Linux | Rust daemon | local whisper.cpp, cloud backends | Vulkan, CUDA, hipblas (build features) | Hyprland, Sway, Niri, GNOME, KDE | layout-aware key injection | tray + overlay | voice-driven LLM commands, read-aloud | listed | 136, 2026-10-10, MIT |
| [Hyprvoice](https://github.com/LeonardoTrapani/hyprvoice) | Linux (Wayland/Hyprland) | Go | 26 models, cloud and whisper.cpp | via whisper.cpp | Hyprland native | ydotool/wtype/clipboard with clipboard restore | notifications | yes (grammar etc.) | not noted | 273, 2026-09-19, MIT |
| [Sayso](https://github.com/watzon/sayso) | Linux, macOS, Windows | Rust + GPUI | Linux: sherpa-onnx (Parakeet, Whisper, SenseVoice, Moonshine, Zipformer); cloud optional | CPU on Linux | X11 and Wayland, per-desktop matrix | paste, then clipboard restore | live preview overlay (waveform, text), history hub | yes: "styles" per app (Clean, Email, ...) | `nix run github:watzon/sayso` | 33, 2026-10-14, GPL-3.0 |
| [Murmure](https://github.com/Kieirra/murmure) | Linux, macOS, Windows | Tauri (TS + Rust) | Parakeet TDT 0.6B v3 only | not emphasised | GlobalShortcuts portal on KDE, Hyprland, Sway | paste into focused app | overlay, history | optional local LLM (translate, grammar) | none found | 1.1k, 2026-10-15, AGPL-3.0 |
| [Speech Note](https://github.com/mkiol/dsnote) | Linux, Sailfish | C++/Qt | whisper.cpp, Faster Whisper, Vosk, Coqui, april-asr; plus TTS and translation | yes | works | notes app, not a paste tool | full GUI | no | Flatpak | 1.7k, 2026-10-12, MPL-2.0 |
| [nerd-dictation](https://github.com/ideasman42/nerd-dictation) | Linux | single Python file | Vosk | CPU | X11 (xdotool) or ydotool | simulated keystrokes | none | none; user Python hook | n/a | 1.9k, 2025-10-10, GPL-3.0 |
| [WhisperWriter](https://github.com/savbell/whisper-writer) | Linux, Windows, macOS | Python + PyQt5 | faster-whisper, OpenAI API | CUDA | X11-oriented | typed input | small status window | none | n/a | 1.1k, 2024-08-24 (stale), GPL-3.0 |
| [BlahST](https://github.com/QuantiusBenignus/BlahST) | Linux | shell scripts | whisper.cpp or whisperfile | yes | via ydotool | ydotool/xdotool | none | local LLM chat and proofreader | n/a | 178, 2025-07-25, BSD-3 |
| [whisper-overlay](https://github.com/oddlama/whisper-overlay) | Linux (Wayland) | Rust | RealtimeSTT server | yes | layer-shell overlay | types text | overlay | none | not checked | 91, 2024-07-26 (stale), MIT |
| Superwhisper (commercial) | macOS, Windows, iOS | native | local Whisper/Parakeet + cloud | Apple silicon | n/a | paste | polished overlay, modes | yes, per-mode prompts | n/a | closed, freemium |
| Wispr Flow (commercial) | macOS, Windows, iOS, Android | native | cloud | n/a | n/a | paste | overlay | yes, central feature | n/a | closed, subscription |

Not covered in depth: [awesome-voice-typing](https://github.com/primaprashant/awesome-voice-typing) lists about 30 more, including VOXD, Speak to AI, OSTT, Freestyle, Voquill and Epicenter Whispering. Newer Linux entries with few stars: shuvoice, hyprvoice-like forks, utter, dikto.

## Overlap (what others already do well)

- **Local whisper.cpp with Vulkan on Linux** is table stakes: Handy, Voxtype, hyprwhspr, Vocalinux, whisrs and OpenWhispr all do it, and several offer more engines.
- **Hyprland / Wayland paste with clipboard restore** exists in Hyprvoice, Sayso, Voxtype (several fallbacks) and hyprwhspr. Only Voxtype documents a plain Ctrl+V paste mode.
- **MPRIS pause while recording**: Voxtype pauses MPRIS players, hyprwhspr ducks audio.
- **Voice-reactive overlay themed from Noctalia/Omarchy**: hyprwhspr does this today; Voxtype has a waveform OSD; Vocalinux an audio-reactive orb.
- **Silence/VAD handling**: Handy uses Silero VAD; token stripping and gating are routine.
- **Toggle mode and external hotkey control**: universal. Most also offer hold-to-talk, which oxivox lacks.
- **Nix**: Handy, Voxtype, Sayso, hyprwhspr, OpenWhispr and Vocalinux ship flakes or Nix packaging. A ready Home Manager service module is rarer.

## Our edge (honest)

Where oxivox is genuinely different:
- **Bluetooth headset call-mode awareness** ("warming up" state while the headset switches profile, leading silence dropped). No README in this survey mentions it. It is a real pain point for headset users and the clearest differentiator, but we only read READMEs, so another tool may handle it quietly.
- **Paste into the window focused when recording started**, via Hyprland IPC `sendshortcut` rather than synthetic keyboard devices (no uinput/udev permissions, no layout issues). Others type via wtype/ydotool into whatever has focus at the end.
- **Latency engineering**: model resident and warmed, about 0.25 s stop-to-text for 10 s of speech. Others claim speed (Voxtype "9-11x realtime", hyprwhspr "instant") but none publishes a comparable stop-to-text figure.
- **Escape cancel through a Hyprland submap**, so no global key grab outside recording.
- **Small and opinionated**: one binary, one engine, no LLM, no history, no cloud.

Where it is not better:
- **One engine only.** Voxtype, hyprwhspr, Handy and Sayso offer Parakeet and newer models (Voxtype claims Cohere Transcribe leads the Open ASR Leaderboard). Parakeet is also much faster on CPU.
- **Hyprland only.** Voxtype, whisrs, Vocalinux and Sayso cover GNOME, KDE, Sway, Niri and X11.
- **No hold-to-talk**, no streaming or live preview (Sayso has it, hyprwhspr experimental), no long-form pause/resume, no meeting mode.
- **No cleanup layer**: no spoken punctuation, replacement tables or custom vocabulary (Voxtype, Sayso, hyprwhspr have them).
- **No configuration UI, tiny community**, one maintainer, no distro packages beyond Nix.
- **Not unique in theming**: hyprwhspr already matches Noctalia themes.

## Ideas worth borrowing

1. **Hold-to-talk mode** (Handy, Voxtype): small change on the existing state machine; make toggle vs hold configurable.
2. **Replacement table and spoken punctuation** (Voxtype, Sayso dictionary): cheap, local, no LLM. Add an initial-prompt vocabulary for whisper to bias names and jargon.
3. **Silero VAD** beside or instead of the energy gate (Handy): better trimming and noise-only rejection.
4. **Engine abstraction with Parakeet via ONNX** (Voxtype, Murmure, Handy): helps low-VRAM or CPU-only machines; keep whisper.cpp default.
5. **Output fallback chain** (Voxtype: wtype, dotool, ydotool, clipboard): a documented clipboard-only fallback when IPC paste fails.
6. **`record start|stop|toggle|cancel|status` CLI plus `copy-last` / `paste-last`** (hyprwhspr): last-transcript recovery is a cheap substitute for a history UI.
7. **Optional post-process hook as a pipe to a user script** (Voxtype): keeps the core LLM-free. Sayso's per-app styles are the heavier version.
8. **Waybar/Noctalia status export** (hyprwhspr): recording and warming state for bars.
9. **Publish a latency benchmark** (stop-to-text on the reference GPU): our strongest story, and nobody else has numbers.

## Sources

Repository metadata via `gh api repos/<owner>/<repo>` and READMEs: [Handy](https://github.com/cjpais/Handy), [OpenWhispr](https://github.com/OpenWhispr/openwhispr), [Voxtype](https://github.com/peteonrails/voxtype), [hyprwhspr](https://github.com/goodroot/hyprwhspr), [Vocalinux](https://github.com/VocaHQ/vocalinux), [whisrs](https://github.com/y0sif/whisrs), [Hyprvoice](https://github.com/LeonardoTrapani/hyprvoice), [Sayso](https://github.com/watzon/sayso), [Murmure](https://github.com/Kieirra/murmure), [Speech Note](https://github.com/mkiol/dsnote), [nerd-dictation](https://github.com/ideasman42/nerd-dictation), [WhisperWriter](https://github.com/savbell/whisper-writer), [BlahST](https://github.com/QuantiusBenignus/BlahST), [whisper-overlay](https://github.com/oddlama/whisper-overlay), [awesome-voice-typing](https://github.com/primaprashant/awesome-voice-typing). Commercial: [Superwhisper](https://superwhisper.com), [Wispr Flow](https://wisprflow.ai).
