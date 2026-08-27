//! Prchum's Linux shell.
//!
//! Same core as the macOS app, a GTK4 and libadwaita presentation on
//! top. What a review *is* — files, hunks, rows, comments, anchors —
//! lives in `prchum-core`; this decides how it looks and how it is
//! driven, in the shapes a GNOME user expects rather than a Mac one.

mod comment;
mod diffview;
mod window;

use adw::prelude::*;
use gtk::glib;

use prchum_core::session::Session;
use prchum_core::source::GitSpec;

const APP_ID: &str = "eu.dumontix.prchum";

fn main() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id(APP_ID)
        // The target is handled here rather than through GTK's own
        // option parsing, which would swallow a bare path.
        .flags(gtk::gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    app.connect_command_line(|app, command_line| {
        let arguments = command_line.arguments();
        let target = arguments
            .iter()
            .skip(1)
            .map(|argument| argument.to_string_lossy().to_string())
            .find(|argument| !argument.starts_with('-'));

        match open(target.as_deref()) {
            Ok(mut session) => {
                // Drafts outlive the window: they go beside the config,
                // in the same place the macOS app keeps them, so a
                // review survives being closed.
                if let Some(dir) = state_dir() {
                    // The forge handle you are known by, which is rarely
                    // the account name; empty falls back to the latter.
                    let config = prchum_core::Config::load(std::path::Path::new(
                        &format!("{dir}/config.json"),
                    ));
                    let author = if config.author().is_empty() {
                        std::env::var("USER").unwrap_or_default()
                    } else {
                        config.author().to_string()
                    };
                    session.set_author(&author);

                    if let Some(warning) = session.attach_store(&dir) {
                        eprintln!("prchum: {warning}");
                    }
                }
                let window = window::build(app, session);
                window.present();
                0
            }
            Err(message) => {
                eprintln!("prchum: {message}");
                1
            }
        }
    });

    app.run()
}

/// Where drafts and configuration live, following the XDG layout rather
/// than the macOS one — same core, each platform's own conventions.
fn state_dir() -> Option<String> {
    let base = match std::env::var("XDG_DATA_HOME") {
        Ok(value) if !value.is_empty() => value,
        _ => format!("{}/.local/share", std::env::var("HOME").ok()?),
    };
    Some(format!("{base}/prchum"))
}

/// A directory is a git repository to compare; anything else is a patch.
///
/// Deliberately narrow for now: the Linux shell opens what it is given
/// and nothing else. Pull requests and the review queue come with the
/// forge adapters, which are already in the core waiting.
fn open(target: Option<&str>) -> Result<Session, String> {
    let Some(target) = target else {
        return Err("give me a patch file or a git repository".to_string());
    };

    let path = std::path::Path::new(target);
    if path.is_dir() {
        Session::from_git(target, &GitSpec::WorkingTree, 3)
            .map_err(|error| format!("could not read {target}: {error}"))
    } else {
        Session::from_patch_file(target)
            .map_err(|error| format!("could not read {target}: {error}"))
    }
}
