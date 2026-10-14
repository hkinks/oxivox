# rustle

Local, private dictation for Linux. Press a key, speak, press it again: the text appears in the app you were typing
in. A small overlay with a row of dots that follow your voice is the only thing on screen.

Speech recognition runs on your own GPU with [whisper.cpp](https://github.com/ggml-org/whisper.cpp) (Whisper
large-v3-turbo). Nothing leaves the machine.

> Status: early development. The first target is Hyprland on NixOS; other desktops follow.

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

- A GPU with Vulkan (CPU fallback planned)
- PipeWire
- The Whisper model `ggml-large-v3-turbo.bin` in `~/.local/share/rustle/models/`

## Usage

```sh
rustle daemon      # usually started by the user service
rustle toggle      # bind this to a key
rustle cancel      # bind to Escape while recording
rustle transcribe recording.wav
```

Hyprland example:

```
bind = ALT, SPACE, exec, rustle toggle
```

## NixOS / Home Manager

The flake exports a Home Manager module that installs the package, runs `rustle daemon` as a systemd user service
and writes the config files:

```nix
{
  inputs.rustle.url = "github:hannes/rustle";   # or a local path

  # in your Home Manager configuration
  imports = [ inputs.rustle.homeManagerModules.default ];

  services.rustle = {
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
| `package` | the flake's package | the rustle package |
| `settings` | `{ }` | `config.toml` (omitted when empty) |
| `theme` | `null` | `theme.toml`; null leaves it to Noctalia or you |
| `hyprland.enable` | `false` | add the binds to `wayland.windowManager.hyprland` and set `settings.cancel_submap = "rustle"` |
| `hyprland.toggleBind` | `"ALT, SPACE"` | the dictation key, also bound inside the `rustle` submap so a recording can be stopped |

The Hyprland integration uses `wayland.windowManager.hyprland.submaps` (Home Manager 26.05 or newer).

## Building

With Nix:

```sh
nix build          # package
nix develop        # dev shell with whisper.cpp, Vulkan and PipeWire build dependencies
```

## Licence

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
