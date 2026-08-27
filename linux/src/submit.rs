//! Sending the review back to the forge.
//!
//! The plan, the ordering and the retry-safety all live in the core;
//! this asks what event the reviewer wants, shows them what is about to
//! leave, and reports what happened.

use adw::prelude::*;
use gtk::glib;

use prchum_core::review::ReviewEvent;
use prchum_forge::open::PrContext;
use prchum_forge::submit;

use crate::comment;

/// What the sheet decided.
pub struct Choice {
    pub event: ReviewEvent,
    pub summary: String,
}

/// Asks what to send, then calls `done` with it. Never calls `done` when
/// the reviewer backs out.
pub fn ask<F>(
    parent: &adw::ApplicationWindow,
    plan: &submit::SubmissionPlan,
    summary: &str,
    done: F,
) where
    F: Fn(Choice) + 'static,
{
    let counts = format!(
        "{} comment{}, {} repl{}",
        plan.review.len(),
        if plan.review.len() == 1 { "" } else { "s" },
        plan.replies.len(),
        if plan.replies.len() == 1 { "y" } else { "ies" },
    );
    let mut body = counts;
    if plan.skipped_dismissed > 0 {
        body.push_str(&format!(
            "\n{} dismissed stay local",
            plan.skipped_dismissed
        ));
    }
    if plan.skipped_orphaned > 0 {
        // Said before the reviewer commits, not after: an orphan is a
        // note whose code is gone, and it is never sent.
        body.push_str(&format!(
            "\n⚠ {} orphaned will NOT be submitted",
            plan.skipped_orphaned
        ));
    }

    let events = gtk::DropDown::from_strings(&["Comment", "Approve", "Request changes"]);
    let summary_view = gtk::TextView::builder()
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(8)
        .right_margin(8)
        .build();
    summary_view.buffer().set_text(summary);

    let scroll = gtk::ScrolledWindow::builder()
        .height_request(140)
        .width_request(460)
        .child(&summary_view)
        .build();
    scroll.add_css_class("card");

    let layout = gtk::Box::new(gtk::Orientation::Vertical, 8);
    layout.append(&events);
    layout.append(&scroll);

    let dialog = adw::AlertDialog::builder()
        .heading("Submit review")
        .body(&body)
        .extra_child(&layout)
        .build();
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("send", "Submit");
    dialog.set_response_appearance("send", adw::ResponseAppearance::Suggested);
    dialog.set_close_response("cancel");

    dialog.connect_response(None, move |_, response| {
        if response != "send" {
            return;
        }
        let event = match events.selected() {
            1 => ReviewEvent::Approve,
            2 => ReviewEvent::RequestChanges,
            _ => ReviewEvent::Comment,
        };
        let buffer = summary_view.buffer();
        let summary = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string();
        done(Choice { event, summary });
    });

    dialog.present(Some(parent));
}

/// Sends, off the main thread, and reports back on it.
///
/// The core removes accepted comments from the draft as they land, so a
/// submission that fails halfway can be retried without posting anything
/// twice — which is exactly why the outcome is applied to the session
/// even when it carries an error.
pub fn send<F>(
    parent: &adw::ApplicationWindow,
    context: &PrContext,
    draft: prchum_core::review::DraftReview,
    done: F,
) where
    F: Fn(submit::SubmitOutcome) + 'static,
{
    let (sender, receiver) = std::sync::mpsc::channel();
    let reference = context.reference.clone();
    let kind = context.kind;
    let template = context.forgejo_template.clone();

    std::thread::spawn(move || {
        let context = PrContext {
            reference: reference.clone(),
            kind,
            forgejo_template: template,
        };
        let plan = submit::plan(&draft);
        let outcome = submit::execute(&*context.forge(), &reference, &draft, &plan);
        let _ = sender.send(outcome);
    });

    let parent = parent.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(120), move || {
        match receiver.try_recv() {
            Ok(outcome) => {
                if let Some(error) = &outcome.error {
                    comment::report(&parent, "Submission stopped", error);
                }
                done(outcome);
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        }
    });
}
