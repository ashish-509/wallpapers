use std::path::PathBuf;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};

const APPLICATION_ID: &str = "io.github.ashish509.Motion.Renderer";

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

        let offload = gtk::GraphicsOffload::new(Some(&picture));
        offload.set_enabled(gtk::GraphicsOffloadEnabled::Enabled);

        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .decorated(false)
            .title(format!("@{APPLICATION_ID}|{index}"))
            .child(&offload)
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
        assert_eq!(APPLICATION_ID, "io.github.ashish509.Motion.Renderer");
    }
}
