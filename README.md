# Motion

Motion is a lightweight live wallpaper manager for GNOME on Wayland. It discovers
freely reusable video from Wikimedia Commons, keeps attribution and license data,
optimizes downloads for desktop playback, and controls a small GNOME Shell renderer.

The native Libadwaita app provides search, previews, favorites, playlists, cache
management, one-click apply, and scheduled rotation. Playback pauses for fullscreen
apps, GameMode, power saver, and low battery without polling. Motion is still a
preview release until its GNOME Shell integration and resource use have been tested
across supported Fedora and GNOME versions.

## Build

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Native builds require Rust 1.85 or newer, GTK 4.14, Libadwaita 1.5, GStreamer with
the GTK4 media backend, and FFmpeg with VP9 encoding support. See
[docs/INSTALL.md](docs/INSTALL.md) for development, RPM, and Flatpak instructions.

The implementation boundaries and Wayland integration are documented in
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Source policy

Automatic discovery currently uses only Wikimedia Commons. Motion accepts public
domain, CC0, CC BY, and CC BY-SA video with complete creator and license metadata.
It rejects unknown licenses and files with declared reuse restrictions. Local
files are not uploaded or automatically assigned reuse rights.

## License

Motion is distributed under GPL-3.0-or-later.