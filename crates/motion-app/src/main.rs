use adw::prelude::*;
use gtk::glib;

mod operations;
mod paths;
mod ui;

const APPLICATION_ID: &str = "io.github.ashish509.Motion";

fn main() -> glib::ExitCode {
    if std::env::args().any(|argument| argument == "--maintenance") {
        return match paths::AppPaths::create().and_then(|paths| operations::run_maintenance(&paths))
        {
            Ok(()) => glib::ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("maintenance failed: {error:#}");
                glib::ExitCode::FAILURE
            }
        };
    }

    let application = adw::Application::builder()
        .application_id(APPLICATION_ID)
        .build();
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
