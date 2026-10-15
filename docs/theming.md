# Theming

The overlay palette comes from `$XDG_CONFIG_HOME/oxivox/theme.toml` (keys `background`, `surface`, `border`, `accent`,
`text`, `error`; every key optional, missing ones use Tokyo Night, see `contrib/theme.toml`). oxivox watches the
directory and applies a changed file at once, also when the writer replaces it by renaming; an invalid file is logged
and the previous palette stays.

## Noctalia

Noctalia (v5) renders user templates whenever the theme changes. `contrib/noctalia/oxivox-theme.toml.tmpl` maps its
Material tokens onto the oxivox palette:

| oxivox | Noctalia token |
|---|---|
| `background` | `surface` |
| `surface` | `surface_container_high` |
| `border` | `outline_variant` |
| `accent` | `primary` |
| `text` | `on_surface` |
| `error` | `error` |

Tokens use the current mode, so the overlay follows light/dark too. Add to `~/.config/noctalia/config.toml`:

```toml
[theme.templates.user.oxivox]
input_path  = "/path/to/oxivox/contrib/noctalia/oxivox-theme.toml.tmpl"
output_path = "$XDG_CONFIG_HOME/oxivox/theme.toml"
```

No `post_hook` is needed because oxivox reloads the file itself. Check the rendering without touching the live
config (`-c` takes a standalone file with `[templates.oxivox]` instead of `[theme.templates.user.oxivox]`):

```sh
noctalia theme some-wallpaper.png --default-mode dark -r contrib/noctalia/oxivox-theme.toml.tmpl:/tmp/oxivox-theme.toml
```
