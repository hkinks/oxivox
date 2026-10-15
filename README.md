# oxivox

Local, private dictation for Linux. Press a key, speak, press it again: the text appears in the app you were typing
in. A small overlay with a row of dots that follow your voice is the only thing on screen.

Speech recognition runs on your own GPU with [whisper.cpp](https://github.com/ggml-org/whisper.cpp) (Whisper
large-v3-turbo). Nothing leaves the machine.

> Status: early development. The first target is Hyprland on NixOS; other desktops follow.

![overlay states: warming up, mic live, silence, speech, error](docs/overlay-states.png)

## Features

- One hotkey: press to start, press again to transcribe and paste. Escape cancels.
- Pastes into the window that was focused when you started, with the right paste shortcut for terminals and GUI apps;
  your clipboard is restored afterwards.
- Overlay: click-through, never takes focus, dots that follow your voice, follows your desktop theme.
- Bluetooth headsets: shows the mic warming up while the headset switches to call mode, so no words get lost.
- Skips silent recordings instead of inventing text.
- Pauses music while you speak.

## Platform support

| desktop | status |
|---|---|
| Hyprland (Wayland) | first target |
| KDE Plasma, GNOME | planned |
| Windows, macOS | later |

Desktop-specific parts (hotkey, overlay window, paste, theme) sit behind small interfaces; the core (audio, model,
overlay drawing) is shared. See [docs/spec.md](docs/spec.md).

## Requirements

- A GPU with Vulkan; the CPU works too, much slower (`gpu = false` in the config)
- PipeWire
- A Wayland compositor with wlr layer-shell; Hyprland for the hotkey, focus and paste integration
- The Whisper model `ggml-large-v3-turbo.bin` (about 1.6 GB) in `~/.local/share/oxivox/models/`:

```sh
mkdir -p ~/.local/share/oxivox/models
curl -L -o ~/.local/share/oxivox/models/ggml-large-v3-turbo.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin
```

## Usage

```sh
oxivox daemon      # usually started by the user service
oxivox toggle      # bind this to a key
oxivox cancel      # bind to Escape while recording
oxivox transcribe recording.wav
```

`oxivox status` prints the daemon's state, `oxivox overlay-demo` shows the overlay with synthetic audio (no model, no
microphone). Hyprland binds, including Escape-to-cancel through a submap, are in
[contrib/hyprland.conf](contrib/hyprland.conf); a systemd user unit is in [contrib/oxivox.service](contrib/oxivox.service).

## Configuration

Every key is optional. `$XDG_CONFIG_HOME/oxivox/config.toml` sets the model path, language, maximum recording length,
GPU use, microphone, overlay size and paste keys per window class; [contrib/config.toml](contrib/config.toml) lists all
keys with their defaults. The overlay colours come from `$XDG_CONFIG_HOME/oxivox/theme.toml`, reloaded on change; see
[docs/theming.md](docs/theming.md) (including a Noctalia template).

## NixOS / Home Manager

The flake exports a Home Manager module that installs the package, runs `oxivox daemon` as a systemd user service
and writes the config files:

```nix
{
  inputs.oxivox.url = "github:hkinks/oxivox";   # or a local path

  # in your Home Manager configuration
  imports = [ inputs.oxivox.homeManagerModules.default ];

  services.oxivox = {
    enable = true;
    settings = { language = "en"; media_ignore = [ "vlc" ]; };   # config.toml
    # theme = { accent = "#7aa2f7"; };                           # theme.toml; null (default) leaves it unmanaged
    hyprland.enable = true;                                      # binds Alt+Space and the Escape submap
  };
}
```

| option | default | meaning |
|---|---|---|
| `enable` | `false` | install the package and run the user service |
| `package` | the flake's package | the oxivox package |
| `settings` | `{ }` | `config.toml` (omitted when empty) |
| `theme` | `null` | `theme.toml`; null leaves it to Noctalia or you |
| `hyprland.enable` | `false` | add the binds to `wayland.windowManager.hyprland` and set `settings.cancel_submap = "oxivox"` |
| `hyprland.toggleBind` | `"ALT, SPACE"` | the dictation key, also bound inside the `oxivox` submap so a recording can be stopped |

The Hyprland integration uses `wayland.windowManager.hyprland.submaps` (Home Manager 26.05 or newer).

## Building

With Nix:

```sh
nix build          # package
nix develop        # dev shell with whisper.cpp, Vulkan and PipeWire build dependencies
```

Without Nix you need a Rust toolchain (edition 2024), cmake, libclang, the Vulkan headers and loader, and the PipeWire,
Wayland and xkbcommon development files; then `cargo build --release`. Tests and project layout are described in
[docs/development.md](docs/development.md), the design in [docs/spec.md](docs/spec.md).

## Licence

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
