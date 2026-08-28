//! Prchum's Linux shell.
//!
//! Same core as the macOS app, a GTK4 and libadwaita presentation on
//! top. What a review *is* — files, hunks, rows, comments, anchors —
//! lives in `prchum-core`; this decides how it looks and how it is
//! driven, in the shapes a GNOME user expects rather than a Mac one.

mod comment;
mod conversation;
mod diffview;
mod queue;
mod settings;
mod submit;
mod threads;
mod window;

use adw::prelude::*;
use gtk::glib;

use prchum_core::session::Session;
use prchum_core::Target;
use prchum_forge::open::{open_session, PrContext};

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
        let words: Vec<String> = arguments
            .iter()
            .skip(1)
            .map(|argument| argument.to_string_lossy().to_string())
            .collect();
        // --staged, spelled both ways git spells it.
        let staged = words
            .iter()
            .any(|word| word == "--staged" || word == "--cached");
        let target = words.iter().find(|word| !word.starts_with('-')).cloned();

        match open(target.as_deref(), staged) {
            Ok((mut session, context)) => {
                // Drafts outlive the window: they go beside the config,
                // in the same place the macOS app keeps them, so a
                // review survives being closed.
                prepare(&mut session);
                let window = window::build(app, session, context);
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

/// Attaches the draft store and settles who drafts belong to.
///
/// Every way in goes through here, so a review opened from the queue
/// persists exactly as one opened from the command line does.
fn prepare(session: &mut Session) {
    let Some(dir) = state_dir() else { return };

    // The forge handle you are known by, which is rarely the account
    // name; empty falls back to the latter.
    let config = prchum_core::Config::load(std::path::Path::new(&format!(
        "{dir}/config.json"
    )));
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

/// Opens a request in a new window, reporting rather than exiting if the
/// forge cannot be reached — the reviewer still has the window they came
/// from.
pub fn open_request(app: &adw::Application, reference: &str) {
    match open(Some(reference), false) {
        Ok((mut session, context)) => {
            prepare(&mut session);
            window::build(app, session, context).present();
        }
        Err(message) => {
            if let Some(parent) = app.active_window() {
                if let Ok(parent) = parent.downcast::<adw::ApplicationWindow>() {
                    comment::report(&parent, "Could not open that request", &message);
                    return;
                }
            }
            eprintln!("prchum: {message}");
        }
    }
}

/// Where drafts and configuration live, following the XDG layout rather
/// than the macOS one — same core, each platform's own conventions.
pub fn state_dir() -> Option<String> {
    let base = match std::env::var("XDG_DATA_HOME") {
        Ok(value) if !value.is_empty() => value,
        _ => format!("{}/.local/share", std::env::var("HOME").ok()?),
    };
    Some(format!("{base}/prchum"))
}

/// Turns what the shell was asked for into a session.
///
/// The reading of the argument is the core's, so `prchum main` means the
/// same thing here as it does on the Mac.
fn open(
    argument: Option<&str>,
    staged: bool,
) -> Result<(Session, Option<PrContext>), String> {
    let cwd = std::env::current_dir()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());

    match prchum_core::target::parse(argument, &cwd, staged) {
        Target::Home => Err(
            "give me a patch file, a git repository, or a pull request (owner/repo#N)"
                .to_string(),
        ),
        Target::File(path) => Session::from_patch_file(&path)
            .map(|session| (session, None))
            .map_err(|error| format!("could not read {path}: {error}")),
        Target::Git { repo, spec } => Session::from_git(&repo, &spec, 3)
            .map(|session| (session, None))
            .map_err(|error| format!("could not read {repo}: {error}")),
        Target::Request(reference) => {
            // The config decides which adapter a host uses, so it is
            // loaded before asking.
            let config = state_dir()
                .map(|dir| {
                    prchum_core::Config::load(std::path::Path::new(&format!(
                        "{dir}/config.json"
                    )))
                })
                .unwrap_or_default();
            open_session(&reference, ".", &config)
                .map(|(session, context)| (session, Some(context)))
        }
    }
}
