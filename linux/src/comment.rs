//! Composing a comment.
//!
//! A dialog with a body field, confirmed with Ctrl+Return the way the
//! rest of GNOME confirms a multi-line entry — Return itself has to keep
//! meaning "new line", because review comments are prose.

use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

/// Opens the composer over `parent`, calling `done` with the body when
/// the reviewer confirms and never when they cancel.
pub fn compose<F>(
    parent: &adw::ApplicationWindow,
    heading: &str,
    initial: &str,
    accept: &str,
    done: F,
) where
    F: Fn(String) + 'static,
{
    let body = gtk::TextView::builder()
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(8)
        .right_margin(8)
        .build();
    body.buffer().set_text(initial);

    let scroll = gtk::ScrolledWindow::builder()
        .height_request(180)
        .width_request(460)
        .child(&body)
        .build();
    scroll.add_css_class("card");

    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body("Ctrl+Return to save.")
        .extra_child(&scroll)
        .build();
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("save", accept);
    dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("save"));
    dialog.set_close_response("cancel");

    let body_text = {
        let body = body.clone();
        move || {
            let buffer = body.buffer();
            buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .to_string()
        }
    };

    // One place decides what saving means; the button and the shortcut
    // both go through it.
    let save = Rc::new({
        let body_text = body_text.clone();
        move || {
            let text = body_text();
            if !text.trim().is_empty() {
                done(text);
            }
        }
    });

    {
        let save = save.clone();
        dialog.connect_response(None, move |_, response| {
            if response == "save" {
                save();
            }
        });
    }

    // Ctrl+Return saves from inside the text view, where the default
    // response cannot reach because Return is busy inserting a newline.
    let controller = gtk::EventControllerKey::new();
    {
        let dialog = dialog.clone();
        let save = save.clone();
        controller.connect_key_pressed(move |_, key, _, modifier| {
            let ctrl = modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            if ctrl && matches!(key, gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter) {
                save();
                dialog.close();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
    }
    body.add_controller(controller);

    // The window's own <Ctrl>Return accelerator would otherwise fire
    // instead of this one and open a second composer on top of the
    // first. Actions that act on the diff are meaningless while a dialog
    // is up, so they are simply switched off for its lifetime.
    suspend_actions(parent, &dialog);

    dialog.present(Some(parent));
    body.grab_focus();
}

/// Actions that must not fire while a dialog is up, disabled until it
/// closes.
const SUSPENDED: &[&str] = &[
    "comment",
    "edit-comment",
    "delete-comment",
    "dismiss-comment",
];

fn suspend_actions(window: &adw::ApplicationWindow, dialog: &adw::AlertDialog) {
    let mut suspended = Vec::new();
    for name in SUSPENDED {
        if let Some(action) = window.lookup_action(name) {
            if let Ok(action) = action.downcast::<gtk::gio::SimpleAction>() {
                action.set_enabled(false);
                suspended.push(action);
            }
        }
    }
    dialog.connect_closed(move |_| {
        for action in &suspended {
            action.set_enabled(true);
        }
    });
}

/// Asks before something irreversible, then does it.
pub fn confirm<F>(parent: &adw::ApplicationWindow, heading: &str, accept: &str, done: F)
where
    F: Fn() + 'static,
{
    let dialog = adw::AlertDialog::builder().heading(heading).build();
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("go", accept);
    dialog.set_response_appearance("go", adw::ResponseAppearance::Destructive);
    dialog.set_close_response("cancel");
    dialog.connect_response(None, move |_, response| {
        if response == "go" {
            done();
        }
    });
    suspend_actions(parent, &dialog);
    dialog.present(Some(parent));
}

/// Says something went wrong, without pretending it did not.
pub fn report(parent: &adw::ApplicationWindow, heading: &str, detail: &str) {
    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(detail)
        .build();
    dialog.add_response("ok", "OK");
    dialog.set_default_response(Some("ok"));
    dialog.present(Some(parent));
}
