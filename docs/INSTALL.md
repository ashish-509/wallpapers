# Installation

Motion targets Fedora GNOME 49 or 50 on Wayland. FFmpeg performs one-time media
optimization; GTK's GStreamer backend performs playback.

## Fedora RPM

Build an offline RPM and source RPM from the current checkout:

```sh
./scripts/build-rpm.sh
sudo dnf install build/rpm/RPMS/*/motion-live-wallpaper-*.rpm
```

Log out and back in after the first install so GNOME Shell discovers the
extension, then enable it:

```sh
gnome-extensions enable motion@ashish509.github.io
systemctl --user enable --now motion-maintenance.timer
```

The RPM preset enables the maintenance timer for new installations. The
explicit `systemctl` command also handles user managers that were already
running during package installation.

## Development checkout

Install Fedora build dependencies:

```sh
sudo dnf install cargo rust gtk4-devel libadwaita-devel \
  gstreamer1-plugin-gtk4 gstreamer1-plugins-good ffmpeg-free glib2-devel nodejs
```

Build and test:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Compile the schema into a temporary directory and launch the app:

```sh
mkdir -p build/schemas
cp gnome-extension/motion@ashish509.github.io/schemas/*.xml build/schemas/
glib-compile-schemas --strict build/schemas
GSETTINGS_SCHEMA_DIR="$PWD/build/schemas" cargo run -p motion-app
```

Install the extension for the current user:

```sh
gnome-extensions pack --force --out-dir build \
  --extra-source=constants.js \
  --extra-source=policy.js \
  --extra-source=renderer.js \
  --extra-source=wallpaper.js \
  gnome-extension/motion@ashish509.github.io
gnome-extensions install --force \
  build/motion@ashish509.github.io.shell-extension.zip
```

Log out and back in before enabling a newly installed extension on Wayland.

## Flatpak

Install the GNOME SDK and Rust extension, then build and install the app:

```sh
flatpak install --user flathub org.gnome.Sdk//50 \
  org.freedesktop.Sdk.Extension.rust-stable//25.08
flatpak-builder --user --install --force-clean build/flatpak \
  packaging/flatpak/io.github.ashish509.motion.yml
```

GNOME Shell extensions run on the host, not inside an application sandbox. Run
the host helper from the same source release to install the extension through
`gnome-extensions`, configure the renderer to use `flatpak run`, and enable the
maintenance timer:

```sh
./scripts/install-flatpak-host.sh
```

Log out and back in after the first extension install. The Flatpak is otherwise
sandboxed to network access, Wayland/X11 display fallback, and GPU rendering. It
stores its catalog and cache below `~/.var/app/io.github.ashish509.motion`.

## Data and removal

Motion stores its SQLite catalog under
`~/.local/share/io.github.ashish509.motion` and media under
`~/.cache/io.github.ashish509.motion`. Removing the package does not delete
either directory. Delete them manually only when their wallpapers, favorites,
playlists, and settings are no longer needed.