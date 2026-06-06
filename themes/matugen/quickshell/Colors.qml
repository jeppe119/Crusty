pragma Singleton

// Crusty colour singleton for Quickshell.
//
// Reads the matugen-generated JSON (see themes/matugen/crusty-quickshell.json)
// and hot-reloads whenever matugen rewrites it on a wallpaper change — no
// Quickshell restart needed (FileView.watchChanges).
//
// Install: copy this file + qmldir into your Quickshell config (e.g.
// ~/.config/quickshell/crusty/), then `import "crusty" as Crusty` and use
// `Crusty.Colors.surface`, `Crusty.Colors.primary`, etc.

import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root

    // Path matugen writes to (matches config.snippet.toml output_path).
    readonly property string colorsPath: Quickshell.env("HOME") + "/.cache/matugen/crusty-colors.json"

    property alias c: adapter

    FileView {
        id: fileView
        path: root.colorsPath
        watchChanges: true
        onFileChanged: reload()
        onLoadFailed: function(error) {
            // Missing file before the first `matugen` run is expected; defaults apply.
        }

        JsonAdapter {
            id: adapter

            property string mode: "dark"

            property color primary: "#d0bcff"
            property color on_primary: "#381e72"
            property color primary_container: "#4f378b"
            property color on_primary_container: "#eaddff"
            property color inverse_primary: "#6750a4"
            property color secondary: "#ccc2dc"
            property color on_secondary: "#332d41"
            property color secondary_container: "#4a4458"
            property color on_secondary_container: "#e8def8"
            property color tertiary: "#efb8c8"
            property color on_tertiary: "#492532"
            property color tertiary_container: "#633b48"
            property color on_tertiary_container: "#ffd8e4"
            property color error: "#f2b8b5"
            property color on_error: "#601410"
            property color error_container: "#8c1d18"
            property color on_error_container: "#f9dedc"
            property color background: "#1c1b1f"
            property color on_background: "#e6e1e5"
            property color surface: "#1c1b1f"
            property color on_surface: "#e6e1e5"
            property color surface_variant: "#49454f"
            property color on_surface_variant: "#cac4d0"
            property color surface_dim: "#141218"
            property color surface_bright: "#3b383e"
            property color surface_container_lowest: "#0f0d13"
            property color surface_container_low: "#1d1b20"
            property color surface_container: "#211f26"
            property color surface_container_high: "#2b2930"
            property color surface_container_highest: "#36343b"
            property color inverse_surface: "#e6e1e5"
            property color inverse_on_surface: "#313033"
            property color outline: "#938f99"
            property color outline_variant: "#49454f"
            property color shadow: "#000000"
            property color scrim: "#000000"
            property color source_color: "#6750a4"
        }
    }

    // Convenience top-level aliases so callers can write `Colors.surface`
    // instead of `Colors.c.surface`.
    readonly property color primary: adapter.primary
    readonly property color on_primary: adapter.on_primary
    readonly property color primary_container: adapter.primary_container
    readonly property color on_primary_container: adapter.on_primary_container
    readonly property color secondary: adapter.secondary
    readonly property color on_secondary: adapter.on_secondary
    readonly property color tertiary: adapter.tertiary
    readonly property color on_tertiary: adapter.on_tertiary
    readonly property color error: adapter.error
    readonly property color background: adapter.background
    readonly property color on_background: adapter.on_background
    readonly property color surface: adapter.surface
    readonly property color on_surface: adapter.on_surface
    readonly property color surface_variant: adapter.surface_variant
    readonly property color on_surface_variant: adapter.on_surface_variant
    readonly property color surface_container: adapter.surface_container
    readonly property color surface_container_high: adapter.surface_container_high
    readonly property color surface_container_highest: adapter.surface_container_highest
    readonly property color outline: adapter.outline
    readonly property color outline_variant: adapter.outline_variant
    readonly property string mode: adapter.mode
}
