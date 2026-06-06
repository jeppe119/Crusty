# Crusty — Matugen theming

System-wide [Material You](https://m3.material.io/styles/color/system/overview)
dynamic colour for Crusty, generated from your wallpaper by
[matugen](https://github.com/InioX/matugen). One `matugen image …` run themes
both the desktop GUI and a Quickshell widget, so Crusty follows your wallpaper.

## What's here

| File | Role |
|------|------|
| `crusty-gui.css` | **matugen template** → CSS custom properties (`--md-sys-color-*`) for the Tauri/Svelte GUI |
| `crusty-quickshell.json` | **matugen template** → JSON colour map for Quickshell |
| `quickshell/Colors.qml` | **Quickshell singleton** that loads the JSON and hot-reloads on change |
| `quickshell/qmldir` | Quickshell module manifest (`import "crusty" as Crusty`) |
| `config.snippet.toml` | `[templates.*]` entries to merge into `~/.config/matugen/config.toml` |

The two `.css`/`.json` files are matugen *templates* (matugen renders them with
`{{ colors.<role>.default.hex }}`). The two `quickshell/*` files are static
*consumers* you copy into your Quickshell config — they read the rendered JSON.

## Setup

1. **Install matugen** (≥ 4.x): `cargo install matugen` or your distro package.

2. **Register the templates** — copy the entries from `config.snippet.toml` into
   `~/.config/matugen/config.toml`, fixing `input_path` to your clone location.

3. **Generate** from your wallpaper:
   ```sh
   matugen image ~/wallpapers/current.jpg          # auto/`-m` mode
   matugen image ~/wallpapers/current.jpg -m dark  # force dark
   ```
   This writes:
   - `~/.config/crusty/theme.css`  → consumed by the GUI
   - `~/.cache/matugen/crusty-colors.json` → consumed by Quickshell

### GUI

The GUI (when built) imports `~/.config/crusty/theme.css` and watches it via
`@tauri-apps/plugin-fs` `watch()`, swapping the stylesheet live. Tailwind v4 maps
the `--md-sys-color-*` custom properties into its theme with `@theme`:

```css
/* app.css */
@import "tailwindcss";
@import url("file:///home/you/.config/crusty/theme.css"); /* or injected at runtime */

@theme {
  --color-primary:    var(--md-sys-color-primary);
  --color-surface:    var(--md-sys-color-surface);
  --color-on-surface: var(--md-sys-color-on-surface);
  /* … */
}
```
Then use `class="bg-surface text-on-surface"` in components.

### Quickshell

Copy the singleton into your Quickshell config:

```sh
mkdir -p ~/.config/quickshell/crusty
cp quickshell/Colors.qml quickshell/qmldir ~/.config/quickshell/crusty/
```

Use it anywhere (it hot-reloads when matugen rewrites the JSON):

```qml
import QtQuick
import "crusty" as Crusty

Rectangle {
    color: Crusty.Colors.surface
    border.color: Crusty.Colors.outline
    Text {
        text: "Crusty"
        color: Crusty.Colors.on_surface
    }
}
```

The singleton ships sensible Material-dark defaults, so it renders correctly
before the first `matugen` run.

## Control surface (bonus)

Crusty exposes **MPRIS2** (`org.mpris.MediaPlayer2.crusty`), so a Quickshell
`Quickshell.Services.Mpris` widget — themed with the colours above — can display
and control playback (play/pause/next/prev/seek), as can Waybar's `mpris` module
and `playerctl -p crusty`. See `ARCHITECTURE.md`.
