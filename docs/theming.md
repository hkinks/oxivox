# Theming

The overlay palette comes from `$XDG_CONFIG_HOME/rustle/theme.toml` (keys `background`, `surface`, `border`, `accent`,
`text`, `error`; every key optional, missing ones use Tokyo Night, see `contrib/theme.toml`). rustle watches the
directory and applies a changed file at once, also when the writer replaces it by renaming; an invalid file is logged
and the previous palette stays.

## Noctalia

Noctalia (v5) renders user templates whenever the theme changes. `contrib/noctalia/rustle-theme.toml.tmpl` maps its
Material tokens onto the rustle palette:

| rustle | Noctalia token |
|---|---|
| `background` | `surface` |
| `surface` | `surface_container_high` |
| `border` | `outline_variant` |
| `accent` | `primary` |
| `text` | `on_surface` |
| `error` | `error` |

Tokens use the current mode, so the overlay follows light/dark too. Add to `~/.config/noctalia/config.toml`:

```toml
[theme.templates.user.rustle]
input_path  = "/path/to/rustle/contrib/noctalia/rustle-theme.toml.tmpl"
output_path = "$XDG_CONFIG_HOME/rustle/theme.toml"
```

No `post_hook` is needed because rustle reloads the file itself. Check the rendering without touching the live
config (`-c` takes a standalone file with `[templates.rustle]` instead of `[theme.templates.user.rustle]`):

```sh
noctalia theme some-wallpaper.png --default-mode dark -r contrib/noctalia/rustle-theme.toml.tmpl:/tmp/rustle-theme.toml
```
