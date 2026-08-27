//! The conversation: comments that belong to the request rather than to
//! a line of it.
//!
//! Separate from the diff because that is what they are — a reply to the
//! change as a whole, not to any part of it — and separate from the
//! review sheet because they post individually rather than riding the
//! atomic review.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use prchum_core::review::GeneralDraft;
use prchum_forge::Comment;

use crate::comment;
use crate::window::Review;

/// Opens the conversation over `parent`. `changed` runs after anything
/// is written, so the caller can update its badge.
pub fn present<F>(
    parent: &adw::ApplicationWindow,
    state: Rc<RefCell<Review>>,
    changed: F,
) where
    F: Fn() + 'static + Clone,
{
    let window = adw::Window::builder()
        .transient_for(parent)
        .modal(false)
        .default_width(720)
        .default_height(560)
        .title("Conversation")
        .build();

    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(vec!["boxed-list".to_string()])
        .build();

    let scroll = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&list)
        .build();
    scroll.set_margin_top(12);
    scroll.set_margin_bottom(12);
    scroll.set_margin_start(12);
    scroll.set_margin_end(12);

    let add = gtk::Button::builder()
        .label("Add comment")
        .css_classes(vec!["suggested-action".to_string()])
        .build();

    let header = adw::HeaderBar::new();
    header.pack_end(&add);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&scroll));
    window.set_content(Some(&toolbar));

    let refill = {
        let list = list.clone();
        let state = state.clone();
        Rc::new(move || fill(&list, &state))
    };
    refill();

    {
        let state = state.clone();
        let refill = refill.clone();
        let changed = changed.clone();
        let parent = parent.clone();
        add.connect_clicked(move |_| {
            let state = state.clone();
            let refill = refill.clone();
            let changed = changed.clone();
            let parent_inner = parent.clone();
            comment::compose(&parent, "Comment on the request", "", "Add", move |body| {
                match state.borrow_mut().session.add_general(body) {
                    Ok(_) => {
                        refill();
                        changed();
                    }
                    Err(error) => {
                        comment::report(&parent_inner, "Could not add that", &error)
                    }
                }
            });
        });
    }

    window.present();
}

/// Rebuilds the list: what the host has, then what you have not sent.
fn fill(list: &gtk::ListBox, state: &Rc<RefCell<Review>>) {
    while let Some(row) = list.first_child() {
        list.remove(&row);
    }

    let inner = state.borrow();
    let posted: Vec<Comment> =
        serde_json::from_str(inner.session.general_json()).unwrap_or_default();
    let drafts: Vec<GeneralDraft> =
        serde_json::from_str(&inner.session.general_drafts_json()).unwrap_or_default();
    drop(inner);

    for entry in &posted {
        let row = adw::ActionRow::builder()
            .title(&format!("@{}", entry.author))
            .subtitle(&one_line(&entry.body))
            .subtitle_lines(4)
            .build();
        list.append(&row);
    }

    for draft in &drafts {
        let row = adw::ActionRow::builder()
            .title("Not sent yet")
            .subtitle(&one_line(&draft.body))
            .subtitle_lines(4)
            .build();
        row.add_css_class("warning");

        // Deleting is the only thing you can do to a draft here; editing
        // it is the composer's job and is not worth a second path.
        let remove = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .valign(gtk::Align::Center)
            .css_classes(vec!["flat".to_string()])
            .build();
        {
            let state = state.clone();
            let list = list.clone();
            let id = draft.local_id.clone();
            remove.connect_clicked(move |_| {
                let _ = state.borrow_mut().session.delete_general(&id);
                fill(&list, &state);
            });
        }
        row.add_suffix(&remove);
        list.append(&row);
    }

    if posted.is_empty() && drafts.is_empty() {
        let row = adw::ActionRow::builder()
            .title("No conversation yet")
            .subtitle("Comments here belong to the request rather than to a line.")
            .build();
        list.append(&row);
    }
}

/// Rows show a summary; the whole body would make the list unreadable.
fn one_line(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
