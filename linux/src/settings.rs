//! Settings, as a GNOME preferences window.
//!
//! Same file underneath as the macOS app writes — `config.json`, whose
//! unknown keys survive every save — but shaped the way GNOME shapes
//! preferences rather than the way AppKit does.

use adw::prelude::*;
use gtk::glib;

use prchum_core::config;
use prchum_core::Config;

/// Opens the window over `parent`. `changed` runs after anything is
/// written, so the caller can repaint what depends on it.
pub fn present<F>(parent: &adw::ApplicationWindow, dir: &str, changed: F)
where
    F: Fn() + 'static + Clone,
{
    let path = format!("{dir}/config.json");
    let config = Config::load(std::path::Path::new(&path));

    let window = adw::PreferencesWindow::builder()
        .transient_for(parent)
        .modal(true)
        .search_enabled(false)
        .build();

    let page = adw::PreferencesPage::new();

    // --- who you are -----------------------------------------------

    let identity = adw::PreferencesGroup::builder()
        .title("Identity")
        .description("Drafts are attributed to this name — your forge handle, which is rarely the account name.")
        .build();

    let author = adw::EntryRow::builder().title("Author").build();
    author.set_text(config.author());
    identity.add(&author);
    page.add(&identity);

    // --- what to review --------------------------------------------

    let discovery = adw::PreferencesGroup::builder()
        .title("Review queue")
        .description("The filter the queue runs when you have not chosen one.")
        .build();

    let filter = adw::EntryRow::builder().title("Default filter").build();
    filter.set_text(config.list_filter());
    discovery.add(&filter);

    let engine = adw::ComboRow::builder().title("Forge").build();
    let engines = gtk::StringList::new(&["gh", "forgejo"]);
    engine.set_model(Some(&engines));
    let current = if config.list_engine() == "forgejo" { 1 } else { 0 };
    engine.set_selected(current);
    discovery.add(&engine);
    page.add(&discovery);

    // --- editing locally -------------------------------------------

    let editing = adw::PreferencesGroup::builder()
        .title("Local editing")
        .description(
            "Opens the file in this editor. {path}, {line} and {dir} are filled in.",
        )
        .build();

    let editor = adw::EntryRow::builder().title("Editor").build();
    editor.set_text(config.editor_command());
    editing.add(&editor);
    page.add(&editing);

    window.add(&page);

    // Written on close rather than on every keystroke: config.json is a
    // file a person also edits by hand, and rewriting it continuously
    // while they type would be rude to whatever else is reading it.
    let path_for_save = path.clone();
    window.connect_close_request(move |_| {
        let write = |key: &str, value: String| {
            let _ = config::set_string(std::path::Path::new(&path_for_save), key, &value);
        };
        write("author", author.text().to_string());
        write("list_filter", filter.text().to_string());
        write(
            "list_engine",
            if engine.selected() == 1 { "forgejo".to_string() } else { "gh".to_string() },
        );
        write("editor_command", editor.text().to_string());
        changed();
        glib::Propagation::Proceed
    });

    window.present();
}
