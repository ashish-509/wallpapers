Name:           motion-live-wallpaper
Version:        0.1.0
Release:        1%{?dist}
Summary:        Licensed live wallpapers for GNOME on Wayland

License:        GPL-3.0-or-later
URL:            https://github.com/ashish-509/wallpapers
Source0:        motion-%{version}.tar.xz

BuildRequires:  cargo >= 1.85
BuildRequires:  rust >= 1.85
BuildRequires:  gcc
BuildRequires:  pkgconfig(gtk4) >= 4.14
BuildRequires:  pkgconfig(libadwaita-1) >= 1.5
BuildRequires:  glib2-devel
BuildRequires:  systemd-rpm-macros
BuildRequires:  desktop-file-utils
BuildRequires:  appstream
Requires:       ffmpeg-free
Requires:       gnome-shell >= 49
Requires:       gstreamer1-plugin-gtk4
Requires:       gstreamer1-plugins-good
Requires:       glib2
Requires(post): glib2
Requires(postun): glib2

%description
Motion discovers freely reusable video from Wikimedia Commons, preserves
creator and license attribution, optimizes media for desktop playback, and
renders it through a small GNOME Shell extension on Wayland.

%prep
%autosetup -n motion-%{version}

%build
cargo build --release --frozen --offline -p motion-app -p motion-renderer

%install
install -Dm0755 target/release/motion-app %{buildroot}%{_bindir}/motion-app
install -Dm0755 target/release/motion-renderer %{buildroot}%{_bindir}/motion-renderer
install -Dm0644 data/io.github.ashish509.motion.desktop \
    %{buildroot}%{_datadir}/applications/io.github.ashish509.motion.desktop
install -Dm0644 data/io.github.ashish509.motion.metainfo.xml \
    %{buildroot}%{_metainfodir}/io.github.ashish509.motion.metainfo.xml
install -Dm0644 data/icons/hicolor/scalable/apps/io.github.ashish509.motion.svg \
    %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/io.github.ashish509.motion.svg
install -Dm0644 gnome-extension/motion@ashish509.github.io/schemas/io.github.ashish509.motion.gschema.xml \
    %{buildroot}%{_datadir}/glib-2.0/schemas/io.github.ashish509.motion.gschema.xml
install -Dm0644 data/systemd/user/motion-maintenance.service \
    %{buildroot}%{_userunitdir}/motion-maintenance.service
install -Dm0644 data/systemd/user/motion-maintenance.timer \
    %{buildroot}%{_userunitdir}/motion-maintenance.timer
install -Dm0644 data/systemd/user-preset/90-motion.preset \
    %{buildroot}%{_userpresetdir}/90-motion.preset
mkdir -p %{buildroot}%{_datadir}/gnome-shell/extensions/motion@ashish509.github.io
cp -a gnome-extension/motion@ashish509.github.io/. \
    %{buildroot}%{_datadir}/gnome-shell/extensions/motion@ashish509.github.io/
rm -rf %{buildroot}%{_datadir}/gnome-shell/extensions/motion@ashish509.github.io/schemas

desktop-file-validate \
    %{buildroot}%{_datadir}/applications/io.github.ashish509.motion.desktop
appstreamcli validate --no-net \
    %{buildroot}%{_metainfodir}/io.github.ashish509.motion.metainfo.xml

%post
/usr/bin/glib-compile-schemas %{_datadir}/glib-2.0/schemas || :
%systemd_user_post motion-maintenance.timer

%preun
%systemd_user_preun motion-maintenance.timer

%postun
/usr/bin/glib-compile-schemas %{_datadir}/glib-2.0/schemas || :
%systemd_user_postun_with_restart motion-maintenance.timer

%files
%license LICENSE
%doc README.md docs/ARCHITECTURE.md docs/INSTALL.md
%{_bindir}/motion-app
%{_bindir}/motion-renderer
%{_datadir}/applications/io.github.ashish509.motion.desktop
%{_metainfodir}/io.github.ashish509.motion.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/io.github.ashish509.motion.svg
%{_datadir}/glib-2.0/schemas/io.github.ashish509.motion.gschema.xml
%{_datadir}/gnome-shell/extensions/motion@ashish509.github.io/
%{_userunitdir}/motion-maintenance.service
%{_userunitdir}/motion-maintenance.timer
%{_userpresetdir}/90-motion.preset

%changelog
* Tue Jul 21 2026 Ashish <ashish509@users.noreply.github.com> - 0.1.0-1
- Initial package