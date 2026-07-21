# Motion

Motion is a lightweight live wallpaper manager for GNOME on Wayland. It discovers
freely reusable video from Wikimedia Commons, keeps attribution and license data,
optimizes downloads for desktop playback, and controls a small GNOME Shell renderer.

The project is under active development. The pure Rust domain and source connector
are implemented first; the native app, service, renderer, and packages follow the
boundaries in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Build

```sh
cargo test --workspace
```

Native builds require GTK4, Libadwaita, GStreamer, FFmpeg, Meson, and Ninja.

## Source policy

Automatic discovery currently uses only Wikimedia Commons. Motion accepts public
domain, CC0, CC BY, and CC BY-SA video with complete creator and license metadata.
It rejects unknown licenses and files with declared reuse restrictions. Local
imports remain local and are never presented as freely licensed media.

## License

Motion is distributed under GPL-3.0-or-later.