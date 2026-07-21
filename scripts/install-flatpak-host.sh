#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
app_id=io.github.ashish509.motion
extension_id=motion@ashish509.github.io
extension_dir="$repo_root/gnome-extension/$extension_id"

for command_name in flatpak gnome-extensions glib-compile-schemas gsettings systemctl; do
    if ! command -v "$command_name" >/dev/null; then
        printf 'required command not found: %s\n' "$command_name" >&2
        exit 1
    fi
done
flatpak info "$app_id" >/dev/null

package_dir=$(mktemp -d)
trap 'rm -rf "$package_dir"' EXIT
gnome-extensions pack --force --out-dir "$package_dir" \
    --extra-source=constants.js \
    --extra-source=policy.js \
    --extra-source=renderer.js \
    --extra-source=wallpaper.js \
    "$extension_dir"
gnome-extensions install --force "$package_dir/$extension_id.shell-extension.zip"

schema_dir="$HOME/.local/share/glib-2.0/schemas"
install -Dm0644 \
    "$extension_dir/schemas/io.github.ashish509.motion.gschema.xml" \
    "$schema_dir/io.github.ashish509.motion.gschema.xml"
glib-compile-schemas --strict "$schema_dir"
GSETTINGS_SCHEMA_DIR="$schema_dir" gsettings set "$app_id" renderer-command \
    "['flatpak', 'run', '--command=motion-renderer', '$app_id']"

unit_dir="$HOME/.config/systemd/user"
install -Dm0644 \
    "$repo_root/packaging/flatpak/motion-maintenance.service" \
    "$unit_dir/motion-maintenance.service"
install -Dm0644 \
    "$repo_root/data/systemd/user/motion-maintenance.timer" \
    "$unit_dir/motion-maintenance.timer"
systemctl --user daemon-reload
systemctl --user enable --now motion-maintenance.timer

if ! gnome-extensions enable "$extension_id"; then
    printf '%s\n' \
        "The extension is installed. Log out and back in, then enable $extension_id."
fi