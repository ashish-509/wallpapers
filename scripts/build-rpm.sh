#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$repo_root/Cargo.toml" | head -n 1)
if [[ -z "$version" ]]; then
    version=0.1.0
fi

output_dir=${1:-"$repo_root/build/rpm"}
work_dir=$(mktemp -d)
trap 'rm -rf "$work_dir"' EXIT

source_dir="$work_dir/motion-$version"
mkdir -p "$source_dir/.cargo" "$output_dir/SOURCES" "$output_dir/SPECS"
tar \
    --exclude=.git \
    --exclude=.flatpak-builder \
    --exclude=build \
    --exclude=target \
    --exclude=vendor \
    -C "$repo_root" \
    -cf - . | tar -C "$source_dir" -xf -

(
    cd "$source_dir"
    cargo vendor --locked vendor > .cargo/config.toml
)
tar -C "$work_dir" -cJf "$output_dir/SOURCES/motion-$version.tar.xz" "motion-$version"
cp "$repo_root/packaging/fedora/motion.spec" "$output_dir/SPECS/motion.spec"

rpmbuild -ba \
    --define "_topdir $output_dir" \
    "$output_dir/SPECS/motion.spec"

find "$output_dir/RPMS" "$output_dir/SRPMS" -type f -name '*.rpm' -print