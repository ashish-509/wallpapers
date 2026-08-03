use adw::prelude::*;
use gtk::{gio, glib};

mod operations;
mod paths;
mod ui;

const APPLICATION_ID: &str = "io.github.ashish509.motion";

fn main() -> glib::ExitCode {
    if std::env::args().any(|argument| argument == "--maintenance") {
        return run_headless(operations::run_maintenance);
    }
    if std::env::args().any(|argument| argument == "--next") {
        return run_headless(|paths| operations::run_step(paths, true));
    }
    if std::env::args().any(|argument| argument == "--prev") {
        return run_headless(|paths| operations::run_step(paths, false));
    }
    if std::env::args().any(|argument| argument == "--toggle") {
        return run_headless(operations::run_toggle);
    }

    let application = adw::Application::builder()
        .application_id(APPLICATION_ID)
        .build();

    // Expose a "quit" action so a global keyboard shortcut can close the running
    // window via `gapplication action io.github.ashish509.motion quit`, and bind
    // it in-app for when the window is focused.
    let quit_action = gio::SimpleAction::new("quit", None);
    let application_ref = application.clone();
    quit_action.connect_activate(move |_, _| application_ref.quit());
    application.add_action(&quit_action);
    application.set_accels_for_action("app.quit", &["<Control><Super>q"]);

    application.connect_activate(|application| {
        if let Some(window) = application.active_window() {
            window.present();
            return;
        }
        if let Err(error) = ui::build(application) {
            eprintln!("application failed: {error:#}");
            application.quit();
        }
    });
    application.run()
}

fn run_headless(task: impl FnOnce(&paths::AppPaths) -> anyhow::Result<()>) -> glib::ExitCode {
    match paths::AppPaths::create().and_then(|paths| task(&paths)) {
        Ok(()) => glib::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("motion-app failed: {error:#}");
            glib::ExitCode::FAILURE
        }
    }
}
