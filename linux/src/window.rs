//! The review window: changed files on the left, the diff on the right.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{Label, ListBox, ListBoxRow, Orientation, PolicyType, ScrolledWindow, TextView};

use prchum_core::diff::{FileDiff, LineKind, Side};
use prchum_core::render::{render_file, RenderedFile, RowKind};
use prchum_core::review::DraftState;
use prchum_core::session::Session;
use prchum_core::syntax;

use crate::comment;
use crate::diffview::{self, Annotation, Note};
use crate::threads;

pub struct Review {
    session: Session,
    /// Present for a pull request: where a submission has to reach.
    context: Option<prchum_forge::open::PrContext>,
    /// Threads the host already has, decoded once when the session opens.
    host_threads: Vec<prchum_forge::ThreadInfo>,
    /// Character offset of each row on screen, for moving the caret.
    offsets: Vec<i32>,
    rendered: RenderedFile,
    current: usize,
}

/// Where the caret is, in the terms a comment is anchored by.
#[derive(Clone, Copy, PartialEq)]
struct Target {
    side: Side,
    line: u32,
}

struct Widgets {
    window: adw::ApplicationWindow,
    view: TextView,
    files: ListBox,
    title: adw::WindowTitle,
    drafts: Label,
}

/// Builds the window and wires it to `session`.
pub fn build(
    app: &adw::Application,
    session: Session,
    context: Option<prchum_forge::open::PrContext>,
) -> adw::ApplicationWindow {
    let title = session.title().to_string();
    let host_threads = threads::decode(session.threads_json());
    let state = Rc::new(RefCell::new(Review {
        session,
        context,
        host_threads,
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
    fill_sidebar(&files, state.borrow().session.files());

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

    let title_widget = adw::WindowTitle::new(&title, "");
    let drafts = Label::new(None);
    drafts.add_css_class("dim-label");

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&title_widget));
    header.pack_end(&drafts);

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

    let widgets = Rc::new(Widgets {
        window: window.clone(),
        view,
        files: files.clone(),
        title: title_widget,
        drafts,
    });

    {
        let state = state.clone();
        let widgets = widgets.clone();
        files.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            show_file(&state, &widgets, row.index() as usize, None);
        });
    }

    // Repaint when the desktop's light/dark setting changes, and once
    // shortly after opening: libadwaita may not have resolved the scheme
    // by the time the first file is painted.
    {
        let state = state.clone();
        let widgets = widgets.clone();
        adw::StyleManager::default().connect_dark_notify(move |_| {
            let index = state.borrow().current;
            show_file(&state, &widgets, index, None);
        });
    }

    files.select_row(files.row_at_index(0).as_ref());
    install_shortcuts(app, &state, &widgets);
    window
}

fn fill_sidebar(files: &ListBox, diffs: &[FileDiff]) {
    for file in diffs {
        let row = ListBoxRow::new();
        let line = gtk::Box::new(Orientation::Horizontal, 8);
        line.set_margin_start(6);
        line.set_margin_end(6);
        line.set_margin_top(3);
        line.set_margin_bottom(3);

        let name = Label::builder()
            .label(file.display_path())
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::Start)
            .build();
        line.append(&name);

        let (added, removed) = counts(file);
        if added > 0 {
            let label = Label::new(Some(&format!("+{added}")));
            label.add_css_class("success");
            line.append(&label);
        }
        if removed > 0 {
            let label = Label::new(Some(&format!("−{removed}")));
            label.add_css_class("error");
            line.append(&label);
        }

        row.set_child(Some(&line));
        files.append(&row);
    }
}

fn counts(file: &FileDiff) -> (usize, usize) {
    let mut added = 0;
    let mut removed = 0;
    for hunk in &file.hunks {
        for line in &hunk.lines {
            match line.kind {
                LineKind::Addition => added += 1,
                LineKind::Deletion => removed += 1,
                _ => {}
            }
        }
    }
    (added, removed)
}

/// Paints file `index`, optionally putting the caret back on a line
/// rather than at an offset.
///
/// The distinction matters after a comment is added or removed: boxes
/// appear and disappear between renders, so a raw offset drifts under
/// the caret while the (side, line) it stood on does not.
fn show_file(
    state: &Rc<RefCell<Review>>,
    widgets: &Rc<Widgets>,
    index: usize,
    restore: Option<Target>,
) {
    let dark = adw::StyleManager::default().is_dark();
    let (file, path) = {
        let state = state.borrow();
        let Some(file) = state.session.files().get(index).cloned() else { return };
        let path = file.display_path().to_string();
        (file, path)
    };

    let highlights = syntax::highlight_file(&file);
    let rendered = render_file(&file, highlights.as_deref());

    let painted = {
        let state = state.borrow();
        let mut annotations: Vec<Annotation> = Vec::new();
        for entry in state
            .session
            .draft()
            .comments
            .iter()
            .filter(|c| c.location.path == path)
        {
            let is_new = entry.location.side == Side::Right;
            if let Some(row) = rendered.row_for(is_new, entry.location.end_line) {
                annotations.push(Annotation { row, note: Note::Draft(entry) });
            }
        }
        // Threads the request already carries, shown in the same column.
        // An outdated one has no current line and simply has nowhere to
        // hang, which is honest: its code is gone.
        for thread in state.host_threads.iter().filter(|t| t.path == path) {
            let Some(line) = thread.line else { continue };
            if let Some(row) = rendered.row_for(threads::is_new_side(thread), line) {
                annotations.push(Annotation { row, note: Note::Thread(thread) });
            }
        }
        annotations.sort_by_key(|a| a.row);
        diffview::paint(&widgets.view, &rendered, &annotations, dark)
    };

    {
        let mut state = state.borrow_mut();
        state.rendered = rendered;
        state.offsets = painted.offsets;
        state.current = index;
    }

    widgets.title.set_subtitle(&path);
    update_badge(state, widgets);

    let buffer = widgets.view.buffer();
    let offset = restore
        .and_then(|target| {
            let state = state.borrow();
            state
                .rendered
                .row_for(target.side == Side::Right, target.line)
                .and_then(|row| state.offsets.get(row).copied())
        })
        .unwrap_or(0);
    buffer.place_cursor(&buffer.iter_at_offset(offset));
    widgets
        .view
        .scroll_to_iter(&mut buffer.iter_at_offset(offset), 0.2, false, 0.0, 0.5);
}

fn update_badge(state: &Rc<RefCell<Review>>, widgets: &Rc<Widgets>) {
    let state = state.borrow();
    let count = state
        .session
        .draft()
        .comments
        .iter()
        .filter(|c| c.state != DraftState::Dismissed)
        .count();
    widgets.drafts.set_label(&match count {
        0 => String::new(),
        1 => "1 draft".to_string(),
        many => format!("{many} drafts"),
    });
}

/// The row the caret sits on, as a side and a line.
///
/// A hunk header stands for no line on either side, so it has no target
/// and commenting there is refused rather than guessed at.
fn caret_target(state: &Rc<RefCell<Review>>, view: &TextView) -> Option<Target> {
    let state = state.borrow();
    let buffer = view.buffer();
    let caret = buffer.iter_at_mark(&buffer.get_insert()).offset();
    let index = state.offsets.iter().rposition(|offset| *offset <= caret)?;
    let row = state.rendered.rows.get(index)?;
    match row.kind {
        RowKind::HunkHeader => None,
        RowKind::Deletion => row.old_line.map(|line| Target { side: Side::Left, line }),
        _ => row
            .new_line
            .map(|line| Target { side: Side::Right, line })
            .or_else(|| row.old_line.map(|line| Target { side: Side::Left, line })),
    }
}

/// The draft anchored at the caret, if there is one.
fn draft_at(state: &Rc<RefCell<Review>>, target: Target) -> Option<String> {
    let state = state.borrow();
    let path = state
        .session
        .files()
        .get(state.current)?
        .display_path()
        .to_string();
    state
        .session
        .draft()
        .comments
        .iter()
        .find(|c| {
            c.location.path == path
                && c.location.side == target.side
                && c.location.start_line <= target.line
                && target.line <= c.location.end_line
        })
        .map(|c| c.local_id.clone())
}

fn add_comment(state: &Rc<RefCell<Review>>, widgets: &Rc<Widgets>) {
    let Some(target) = caret_target(state, &widgets.view) else {
        comment::report(
            &widgets.window,
            "Put the cursor on a line of the diff",
            "A hunk header does not stand for a line in either file.",
        );
        return;
    };

    let side = if target.side == Side::Right { "RIGHT" } else { "LEFT" };
    let heading = format!("Comment on line {} ({side})", target.line);
    let state = state.clone();
    let widgets = widgets.clone();
    let parent = widgets.window.clone();
    comment::compose(&parent, &heading, "", "Comment", move |body| {
        let index = state.borrow().current;
        let result =
            state
                .borrow_mut()
                .session
                .add_comment(index, target.side, target.line, target.line, body);
        match result {
            Ok(_) => show_file(&state, &widgets, index, Some(target)),
            Err(error) => comment::report(&widgets.window, "Could not add that comment", &error),
        }
    });
}

fn edit_comment(state: &Rc<RefCell<Review>>, widgets: &Rc<Widgets>) {
    let Some(target) = caret_target(state, &widgets.view) else { return };
    let Some(id) = draft_at(state, target) else {
        comment::report(
            &widgets.window,
            "No draft here",
            "Put the cursor on a line you have commented on.",
        );
        return;
    };
    let existing = {
        let state = state.borrow();
        state
            .session
            .draft()
            .comments
            .iter()
            .find(|c| c.local_id == id)
            .map(|c| c.body.clone())
            .unwrap_or_default()
    };

    let state = state.clone();
    let widgets = widgets.clone();
    let parent = widgets.window.clone();
    comment::compose(&parent, "Edit comment", &existing, "Save", move |body| {
        let index = state.borrow().current;
        let result = state.borrow_mut().session.update_comment(&id, body);
        match result {
            Ok(()) => show_file(&state, &widgets, index, Some(target)),
            Err(error) => comment::report(&widgets.window, "Could not save that", &error),
        }
    });
}

fn delete_comment(state: &Rc<RefCell<Review>>, widgets: &Rc<Widgets>) {
    let Some(target) = caret_target(state, &widgets.view) else { return };
    let Some(id) = draft_at(state, target) else { return };

    let state = state.clone();
    let widgets = widgets.clone();
    let parent = widgets.window.clone();
    comment::confirm(&parent, "Delete this draft?", "Delete", move || {
        let index = state.borrow().current;
        let result = state.borrow_mut().session.delete_comment(&id);
        match result {
            Ok(()) => show_file(&state, &widgets, index, Some(target)),
            Err(error) => comment::report(&widgets.window, "Could not delete that", &error),
        }
    });
}

fn dismiss_comment(state: &Rc<RefCell<Review>>, widgets: &Rc<Widgets>) {
    let Some(target) = caret_target(state, &widgets.view) else { return };
    let Some(id) = draft_at(state, target) else { return };
    let index = state.borrow().current;
    let result = state.borrow_mut().session.toggle_dismiss(&id);
    match result {
        Ok(()) => show_file(state, widgets, index, Some(target)),
        Err(error) => comment::report(&widgets.window, "Could not dismiss that", &error),
    }
}

fn submit_review(state: &Rc<RefCell<Review>>, widgets: &Rc<Widgets>) {
    let has_context = state.borrow().context.is_some();
    if !has_context {
        comment::report(
            &widgets.window,
            "Submitting needs a pull request",
            "A patch or a git comparison has nowhere to send a review; \
             export the notes instead.",
        );
        return;
    }

    let (plan, summary) = {
        let state = state.borrow();
        (
            prchum_forge::submit::plan(state.session.draft()),
            state.session.draft().summary.clone(),
        )
    };
    if plan.is_empty() && summary.is_empty() {
        comment::report(
            &widgets.window,
            "Nothing to submit",
            "There are no drafts waiting to go out.",
        );
        return;
    }

    let state = state.clone();
    let widgets = widgets.clone();
    let parent = widgets.window.clone();
    crate::submit::ask(&parent, &plan, &summary, move |choice| {
        let draft = {
            let mut inner = state.borrow_mut();
            inner.session.draft_mut().event = choice.event;
            inner.session.draft_mut().summary = choice.summary.clone();
            inner.session.draft().clone()
        };
        let Some(context) = state.borrow().context.clone() else { return };

        let state = state.clone();
        let widgets = widgets.clone();
        crate::submit::send(&widgets.window.clone(), &context, draft, move |outcome| {
            let complete = outcome.error.is_none();
            let posted = outcome.accepted.len();
            let index = state.borrow().current;
            let remaining = state
                .borrow_mut()
                .session
                .apply_accepted(&outcome.accepted, complete)
                .unwrap_or(0);
            show_file(&state, &widgets, index, None);
            if complete {
                comment::report(
                    &widgets.window,
                    "Review submitted",
                    &format!(
                        "{posted} comment{} posted, {remaining} still local.",
                        if posted == 1 { "" } else { "s" }
                    ),
                );
            }
        });
    });
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
        let offset = state.offsets[row];
        buffer.place_cursor(&buffer.iter_at_offset(offset));
        view.scroll_to_iter(&mut buffer.iter_at_offset(offset), 0.2, false, 0.0, 0.5);
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
    app: &adw::Application,
    state: &Rc<RefCell<Review>>,
    widgets: &Rc<Widgets>,
) {
    // Actions with accelerators rather than a key controller: a
    // controller on the window sits behind the focused widget, and the
    // text view swallows the arrows before it ever sees them. Actions
    // also give the names their own identity, which is what the `keys`
    // map in the config talks about.
    //
    // Ctrl-shaped chords, not the macOS Command ones. The names are
    // shared between the shells; the chords belong to the platform.
    let actions: Vec<(&str, &[&str], Box<dyn Fn()>)> = vec![
        ("next-change", &["<Ctrl>Down"], {
            let (state, widgets) = (state.clone(), widgets.clone());
            Box::new(move || step_change(&state, &widgets.view, true))
        }),
        ("prev-change", &["<Ctrl>Up"], {
            let (state, widgets) = (state.clone(), widgets.clone());
            Box::new(move || step_change(&state, &widgets.view, false))
        }),
        ("next-file", &["<Ctrl><Shift>Down"], {
            let (state, widgets) = (state.clone(), widgets.clone());
            Box::new(move || step_file(&state, &widgets.files, true))
        }),
        ("prev-file", &["<Ctrl><Shift>Up"], {
            let (state, widgets) = (state.clone(), widgets.clone());
            Box::new(move || step_file(&state, &widgets.files, false))
        }),
        ("comment", &["<Ctrl>Return"], {
            let (state, widgets) = (state.clone(), widgets.clone());
            Box::new(move || add_comment(&state, &widgets))
        }),
        ("edit-comment", &["<Ctrl>e"], {
            let (state, widgets) = (state.clone(), widgets.clone());
            Box::new(move || edit_comment(&state, &widgets))
        }),
        ("delete-comment", &["<Ctrl>Delete", "<Ctrl>BackSpace"], {
            let (state, widgets) = (state.clone(), widgets.clone());
            Box::new(move || delete_comment(&state, &widgets))
        }),
        ("submit", &["<Ctrl><Shift>Return"], {
            let (state, widgets) = (state.clone(), widgets.clone());
            Box::new(move || submit_review(&state, &widgets))
        }),
        ("dismiss-comment", &["<Ctrl><Shift>x"], {
            let (state, widgets) = (state.clone(), widgets.clone());
            Box::new(move || dismiss_comment(&state, &widgets))
        }),
    ];

    for (name, accels, run) in actions {
        let action = gtk::gio::SimpleAction::new(name, None);
        action.connect_activate(move |_, _| run());
        widgets.window.add_action(&action);
        app.set_accels_for_action(&format!("win.{name}"), accels);
    }
}
