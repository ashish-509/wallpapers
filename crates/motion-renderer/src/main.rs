use std::path::PathBuf;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};

const APPLICATION_ID: &str = "io.github.ashish509.motion.Renderer";

fn main() -> glib::ExitCode {
    let Some(path) = wallpaper_path() else {
        eprintln!("usage: motion-renderer --file PATH");
        return glib::ExitCode::FAILURE;
    };
    if !path.is_file() {
        eprintln!("wallpaper does not exist: {}", path.display());
        return glib::ExitCode::FAILURE;
    }

    let application = gtk::Application::builder()
        .application_id(APPLICATION_ID)
        .build();
    application.connect_activate(move |application| build_windows(application, &path));
    application.run_with_args::<&str>(&[])
}

fn wallpaper_path() -> Option<PathBuf> {
    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--file" {
            return arguments.next().map(PathBuf::from);
        }
    }
    None
}

fn build_windows(application: &gtk::Application, path: &PathBuf) {
    if !application.windows().is_empty() {
        return;
    }

    let Some(display) = gdk::Display::default() else {
        application.quit();
        return;
    };
    let media = gtk::MediaFile::for_filename(path);
    media.set_loop(true);
    media.set_muted(true);

    let pause = gio::SimpleAction::new("pause", None);
    pause.connect_activate({
        let media = media.clone();
        move |_, _| media.pause()
    });
    application.add_action(&pause);

    let play = gio::SimpleAction::new("play", None);
    play.connect_activate({
        let media = media.clone();
        move |_, _| media.play()
    });
    application.add_action(&play);

    // Swap the wallpaper in place so the extension never has to relaunch the
    // renderer on rotation; relaunching left the static desktop wallpaper
    // visible during the gap between the old and new renderer windows.
    let set_file = gio::SimpleAction::new("set-file", Some(glib::VariantTy::STRING));
    set_file.connect_activate({
        let media = media.clone();
        move |_, parameter| {
            let Some(path) = parameter.and_then(glib::Variant::str) else {
                return;
            };
            if !PathBuf::from(path).is_file() {
                return;
            }
            media.set_filename(Some(path));
            media.set_loop(true);
            media.set_muted(true);
            media.play();
        }
    });
    application.add_action(&set_file);

    let monitors = display.monitors();
    for index in 0..monitors.n_items() {
        let Some(monitor) = monitors
            .item(index)
            .and_then(|item| item.downcast::<gdk::Monitor>().ok())
        else {
            continue;
        };
        let picture = gtk::Picture::for_paintable(&media);
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_can_shrink(true);

        // The GNOME extension hides this window (minimized) and clones its actor
        // onto the desktop background. GraphicsOffload delegates the video to a
        // dedicated Wayland subsurface for direct scan-out, which the compositor
        // stops updating while the window is hidden and which the background clone
        // cannot reproduce, leaving a frozen frame. Compositing the video into the
        // window's own buffer lets Mutter keep it rendering via its cloned actor.
        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .decorated(false)
            .title(format!("@{APPLICATION_ID}|{index}"))
            .child(&picture)
            .build();
        window.fullscreen_on_monitor(&monitor);
        window.present();
    }
    media.play();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_id_is_stable_for_shell_matching() {
        assert_eq!(APPLICATION_ID, "io.github.ashish509.motion.Renderer");
    }
}
