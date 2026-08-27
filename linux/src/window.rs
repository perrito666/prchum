//! The review window: changed files on the left, the diff on the right.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use gtk::{Label, ListBox, ListBoxRow, Orientation, PolicyType, ScrolledWindow, TextView};

use prchum_core::render::{render_file, RenderedFile};
use prchum_core::session::Session;
use prchum_core::syntax;

pub struct Review {
    session: Session,
    /// Row offsets of the file on screen, for moving the caret by row.
    offsets: Vec<i32>,
    rendered: RenderedFile,
    current: usize,
}

/// Builds the window and wires it to `session`.
pub fn build(app: &adw::Application, session: Session) -> adw::ApplicationWindow {
    let title = session.title().to_string();
    let state = Rc::new(RefCell::new(Review {
        session,
        offsets: Vec::new(),
        rendered: RenderedFile::default(),
        current: 0,
    }));

    let view = TextView::builder()
        .editable(false)
        .monospace(true)
        .cursor_visible(true)
        .left_margin(8)
        .top_margin(6)
        .build();

    let diff_scroll = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Automatic)
        .vscrollbar_policy(PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .child(&view)
        .build();

    let files = ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .css_classes(vec!["navigation-sidebar".to_string()])
        .build();

    for file in state.borrow().session.files() {
        let row = ListBoxRow::new();
        let box_ = gtk::Box::new(Orientation::Horizontal, 8);
        box_.set_margin_start(6);
        box_.set_margin_end(6);
        box_.set_margin_top(3);
        box_.set_margin_bottom(3);

        let name = Label::builder()
            .label(file.display_path())
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::Start)
            .build();
        box_.append(&name);

        let (added, removed) = counts(file);
        if added > 0 {
            let label = Label::new(Some(&format!("+{added}")));
            label.add_css_class("success");
            box_.append(&label);
        }
        if removed > 0 {
            let label = Label::new(Some(&format!("−{removed}")));
            label.add_css_class("error");
            box_.append(&label);
        }

        row.set_child(Some(&box_));
        files.append(&row);
    }

    let sidebar = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .width_request(260)
        .child(&files)
        .build();

    let split = gtk::Paned::builder()
        .orientation(Orientation::Horizontal)
        .start_child(&sidebar)
        .end_child(&diff_scroll)
        .position(260)
        .resize_start_child(false)
        .build();

    let header = adw::HeaderBar::new();
    let title_widget = adw::WindowTitle::new(&title, "");
    header.set_title_widget(Some(&title_widget));

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&split));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .default_width(1180)
        .default_height(720)
        .content(&toolbar)
        .build();
    // The window manager's title, which is not the header bar's: it is
    // what appears in the overview and what a screenshot script finds a
    // window by.
    window.set_title(Some(&title));

    // Selecting a file paints it.
    {
        let state = state.clone();
        let view = view.clone();
        let title_widget = title_widget.clone();
        files.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            let index = row.index() as usize;
            show_file(&state, &view, index);
            let state = state.borrow();
            if let Some(file) = state.session.files().get(index) {
                title_widget.set_subtitle(file.display_path());
            }
        });
    }

    files.select_row(files.row_at_index(0).as_ref());
    install_shortcuts(&window, &state, &view, &files);
    window
}

fn counts(file: &prchum_core::diff::FileDiff) -> (usize, usize) {
    let mut added = 0;
    let mut removed = 0;
    for hunk in &file.hunks {
        for line in &hunk.lines {
            match line.kind {
                prchum_core::diff::LineKind::Addition => added += 1,
                prchum_core::diff::LineKind::Deletion => removed += 1,
                _ => {}
            }
        }
    }
    (added, removed)
}

fn show_file(state: &Rc<RefCell<Review>>, view: &TextView, index: usize) {
    let dark = adw::StyleManager::default().is_dark();
    let mut state = state.borrow_mut();
    let Some(file) = state.session.files().get(index).cloned() else { return };

    let highlights = syntax::highlight_file(&file);
    let rendered = render_file(&file, highlights.as_deref());
    let offsets = crate::diffview::paint(view, &rendered, dark);

    state.rendered = rendered;
    state.offsets = offsets;
    state.current = index;

    // Start at the top of the file rather than wherever the last one
    // left the caret.
    let buffer = view.buffer();
    buffer.place_cursor(&buffer.start_iter());
}

/// Moves the caret to the next or previous row that is part of a change.
fn step_change(state: &Rc<RefCell<Review>>, view: &TextView, forward: bool) {
    let state = state.borrow();
    if state.offsets.is_empty() {
        return;
    }
    let buffer = view.buffer();
    let caret = buffer.iter_at_mark(&buffer.get_insert()).offset();
    let here = state
        .offsets
        .iter()
        .rposition(|offset| *offset <= caret)
        .unwrap_or(0);

    let candidate = if forward {
        (here + 1..state.rendered.rows.len()).find(|i| state.rendered.rows[*i].is_change())
    } else {
        (0..here).rev().find(|i| state.rendered.rows[*i].is_change())
    };

    if let Some(row) = candidate {
        let iter = buffer.iter_at_offset(state.offsets[row]);
        buffer.place_cursor(&iter);
        view.scroll_to_iter(&mut buffer.iter_at_offset(state.offsets[row]), 0.2, false, 0.0, 0.5);
    }
}

fn step_file(state: &Rc<RefCell<Review>>, files: &ListBox, forward: bool) {
    let (current, total) = {
        let state = state.borrow();
        (state.current, state.session.files().len())
    };
    if total == 0 {
        return;
    }
    let next = if forward {
        (current + 1).min(total - 1)
    } else {
        current.saturating_sub(1)
    };
    files.select_row(files.row_at_index(next as i32).as_ref());
}

fn install_shortcuts(
    window: &adw::ApplicationWindow,
    state: &Rc<RefCell<Review>>,
    view: &TextView,
    files: &ListBox,
) {
    // Ctrl-shaped rather than the macOS Command shapes: the action names
    // are shared with the other shell, the chords are not.
    let controller = gtk::EventControllerKey::new();
    let state = state.clone();
    let view = view.clone();
    let files = files.clone();
    controller.connect_key_pressed(move |_, key, _, modifier| {
        let ctrl = modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK);
        let shift = modifier.contains(gtk::gdk::ModifierType::SHIFT_MASK);
        if !ctrl {
            return glib::Propagation::Proceed;
        }
        match key {
            gtk::gdk::Key::Down if shift => step_file(&state, &files, true),
            gtk::gdk::Key::Up if shift => step_file(&state, &files, false),
            gtk::gdk::Key::Down => step_change(&state, &view, true),
            gtk::gdk::Key::Up => step_change(&state, &view, false),
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    });
    window.add_controller(controller);
}
