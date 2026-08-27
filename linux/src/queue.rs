//! The review queue: what the forge says is waiting for you.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use prchum_core::Config;
use prchum_forge::forgejo::ForgejoForge;
use prchum_forge::ghcli::ProcessRunner;
use prchum_forge::list::{self, ListedRequest};

use crate::comment;

/// Asks the configured forge, off the main thread, then calls `chosen`
/// with whatever the reviewer picks.
pub fn present<F>(parent: &adw::ApplicationWindow, config: &Config, chosen: F)
where
    F: Fn(String) + 'static,
{
    let engine = config.list_engine().to_string();
    let filter = config.list_filter().to_string();
    let host = config.list_host().to_string();
    let template = config.forgejo_api_command().to_string();

    let window = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .default_width(820)
        .default_height(480)
        .title("Review queue")
        .build();

    let spinner = gtk::Spinner::new();
    spinner.start();
    spinner.set_size_request(32, 32);
    let status = adw::StatusPage::builder()
        .title("Asking the forge")
        .child(&spinner)
        .build();

    let stack = gtk::Stack::new();
    stack.add_named(&status, Some("loading"));

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&stack));
    window.set_content(Some(&toolbar));
    window.present();

    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // Only two engines can answer this question today; GitLab has an
        // adapter but no listing, so it is not offered rather than
        // offered and broken.
        let result = if engine == "forgejo" {
            let forge = ForgejoForge::with_runner(ProcessRunner, &template);
            list::list_forgejo(&forge, &host, &filter)
        } else {
            list::list_github(&ProcessRunner, &filter)
        };
        let _ = sender.send(result);
    });

    let chosen = Rc::new(chosen);
    let window_ref = window.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return glib::ControlFlow::Break
            }
        };

        match result {
            Ok(requests) if requests.is_empty() => {
                stack.add_named(
                    &adw::StatusPage::builder()
                        .title("Nothing waiting")
                        .description("No requests match the filter.")
                        .build(),
                    Some("empty"),
                );
                stack.set_visible_child_name("empty");
            }
            Ok(requests) => {
                let list = build_list(&requests, {
                    let chosen = chosen.clone();
                    let window = window_ref.clone();
                    move |reference| {
                        window.close();
                        chosen(reference);
                    }
                });
                stack.add_named(&list, Some("list"));
                stack.set_visible_child_name("list");
            }
            Err(error) => {
                window_ref.close();
                // The forge CLI is how prchum reaches a host, so its
                // absence is the likeliest cause and worth saying.
                comment::report(
                    &parent_of(&window_ref),
                    "Could not ask the forge",
                    &error,
                );
            }
        }
        glib::ControlFlow::Break
    });
}

/// How to name a request when reopening it. The URL is preferred when
/// the host has one, because it carries the host; `owner/repo#n` alone
/// would be read as github.com.
fn reference_for(request: &ListedRequest) -> String {
    if request.url.is_empty() {
        format!("{}/{}#{}", request.owner, request.repo, request.number)
    } else {
        request.url.clone()
    }
}

/// The window's parent, for reporting after it has closed.
fn parent_of(window: &adw::Window) -> adw::ApplicationWindow {
    window
        .transient_for()
        .and_then(|parent| parent.downcast::<adw::ApplicationWindow>().ok())
        .expect("the queue is always shown over a review window")
}

fn build_list<F>(requests: &[ListedRequest], open: F) -> gtk::Widget
where
    F: Fn(String) + 'static,
{
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .css_classes(vec!["boxed-list".to_string()])
        .build();

    let references: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    for request in requests {
        let reference = reference_for(request);
        let row = adw::ActionRow::builder()
            .title(&request.title)
            .subtitle(&format!(
                "{reference} · @{} · {}",
                request.author,
                request.updated_at.split('T').next().unwrap_or("")
            ))
            .activatable(true)
            .build();
        list.append(&row);
        references.borrow_mut().push(reference);
    }

    {
        let references = references.clone();
        list.connect_row_activated(move |_, row| {
            let index = row.index() as usize;
            if let Some(reference) = references.borrow().get(index) {
                open(reference.clone());
            }
        });
    }

    let scroll = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&list)
        .build();
    scroll.set_margin_top(12);
    scroll.set_margin_bottom(12);
    scroll.set_margin_start(12);
    scroll.set_margin_end(12);
    scroll.upcast()
}
