//! Painting a rendered file into a text view.
//!
//! The core decides what the rows are; this decides what they look like.
//! Nothing here works out which line a row stands for — that arrives
//! already settled in [`RenderedFile`], which is the whole point of the
//! core owning it.

use gtk::prelude::*;
use gtk::{TextBuffer, TextTag, TextView};

use prchum_core::render::{RenderedFile, RowKind};
use prchum_core::review::DraftComment;
use prchum_core::syntax;

/// Width of the two line-number columns, which keeps the marker column
/// and the code aligned down the file.
const NUMBER_WIDTH: usize = 5;

/// A draft shown inline, and the row it hangs under.
pub struct Annotation<'a> {
    pub row: usize,
    pub comment: &'a DraftComment,
}

/// Where the caret can be, and what it points at.
pub struct Painted {
    /// Character offset of each row of the diff, indexed as the core's
    /// rows are. Comment boxes are not rows and do not appear here.
    pub offsets: Vec<i32>,
}

fn rgba(color: u32) -> String {
    // Core colours are 0xRRGGBBAA; alpha is carried but always opaque in
    // the shipped themes, so a hex string is enough.
    format!(
        "#{:02x}{:02x}{:02x}",
        (color >> 24) & 0xff,
        (color >> 16) & 0xff,
        (color >> 8) & 0xff
    )
}

/// Creates the tags a rendered file needs: one per syntax style, plus
/// the row tints, the gutter and the comment box.
///
/// `dark` picks which half of the core's style table to use. It is
/// passed in rather than read here so the caller can rebuild the buffer
/// when libadwaita reports the system scheme changing.
fn install_tags(buffer: &TextBuffer, dark: bool) {
    let table = buffer.tag_table();

    let gutter = TextTag::builder()
        .name("gutter")
        .foreground(if dark { "#6b7280" } else { "#9ca3af" })
        .build();
    table.add(&gutter);

    // Tints sit behind the whole row, marker column included, so a
    // change reads as a band rather than as coloured text.
    let addition = TextTag::builder()
        .name("addition")
        .background(if dark { "#12321f" } else { "#e6f4ea" })
        .build();
    table.add(&addition);

    let deletion = TextTag::builder()
        .name("deletion")
        .background(if dark { "#3a1720" } else { "#fce8e6" })
        .build();
    table.add(&deletion);

    let header = TextTag::builder()
        .name("header")
        .background(if dark { "#1f2937" } else { "#eef2ff" })
        .foreground(if dark { "#9ca3af" } else { "#4b5563" })
        .build();
    table.add(&header);

    // A draft reads as a quiet block rather than as more diff: a wash
    // that works in both appearances without fighting the tints.
    let comment = TextTag::builder()
        .name("comment")
        .background(if dark { "#242a36" } else { "#f1f3f5" })
        .build();
    table.add(&comment);

    let byline = TextTag::builder()
        .name("byline")
        .foreground(if dark { "#e0904a" } else { "#a1571c" })
        .weight(700)
        .build();
    table.add(&byline);

    let dismissed = TextTag::builder()
        .name("dismissed")
        .strikethrough(true)
        .foreground(if dark { "#6b7280" } else { "#9ca3af" })
        .build();
    table.add(&dismissed);

    for (index, style) in syntax::styles().iter().enumerate() {
        let colour = if dark { style.dark } else { style.light };
        let tag = TextTag::builder()
            .name(format!("syn{index}"))
            .foreground(&rgba(colour))
            .build();
        if style.flags & syntax::STYLE_BOLD != 0 {
            tag.set_weight(700);
        }
        if style.flags & syntax::STYLE_ITALIC != 0 {
            tag.set_style(gtk::pango::Style::Italic);
        }
        table.add(&tag);
    }
}

fn number(value: Option<u32>) -> String {
    match value {
        Some(line) => format!("{line:>NUMBER_WIDTH$}"),
        None => " ".repeat(NUMBER_WIDTH),
    }
}

/// The blank gutter a comment box is indented by, so its text lines up
/// with the code rather than the line numbers.
fn indent() -> String {
    " ".repeat(NUMBER_WIDTH * 2 + 3)
}

/// How wide a comment body runs before it is folded onto the next line.
///
/// The indent is spent before the first character, so this is the width
/// of the text itself rather than of the line it sits on.
const BODY_WIDTH: usize = 72;

/// Breaks `line` on word boundaries, never mid-word unless a single word
/// is longer than the whole width.
fn wrap(line: &str, width: usize) -> Vec<String> {
    if line.chars().count() <= width {
        return vec![line.to_string()];
    }
    let mut out = Vec::new();
    let mut current = String::new();
    for word in line.split_whitespace() {
        let candidate = current.chars().count() + 1 + word.chars().count();
        if !current.is_empty() && candidate > width {
            out.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        out.push(current);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn insert_comment(buffer: &TextBuffer, comment: &DraftComment) {
    let start_offset = buffer.end_iter().offset();
    let when = comment.at.split('T').next().unwrap_or("").to_string();
    let author = if comment.author.is_empty() {
        "you".to_string()
    } else {
        format!("@{}", comment.author)
    };

    let mut end = buffer.end_iter();
    buffer.insert(&mut end, &format!("{}{author}", indent()));
    let byline_end = buffer.end_iter().offset();
    buffer.apply_tag_by_name(
        "byline",
        &buffer.iter_at_offset(start_offset),
        &buffer.iter_at_offset(byline_end),
    );

    let mut end = buffer.end_iter();
    if when.is_empty() {
        buffer.insert(&mut end, "\n");
    } else {
        buffer.insert(&mut end, &format!("  ·  {when}\n"));
    }

    // The body is shown as written — no Markdown rendering yet, so
    // nothing is silently swallowed — but wrapped, because the view
    // itself must not wrap: code has to keep its columns.
    for line in comment.body.lines() {
        for wrapped in wrap(line, BODY_WIDTH) {
            let mut end = buffer.end_iter();
            buffer.insert(&mut end, &format!("{}{wrapped}\n", indent()));
        }
    }

    let box_start = buffer.iter_at_offset(start_offset);
    let box_end = buffer.end_iter();
    buffer.apply_tag_by_name("comment", &box_start, &box_end);
    if comment.state == prchum_core::review::DraftState::Dismissed {
        buffer.apply_tag_by_name(
            "dismissed",
            &buffer.iter_at_offset(start_offset),
            &buffer.end_iter(),
        );
    }
}

/// Fills `view` with `file` and its drafts, returning where each row of
/// the diff starts.
pub fn paint(
    view: &TextView,
    file: &RenderedFile,
    annotations: &[Annotation],
    dark: bool,
) -> Painted {
    let buffer = view.buffer();
    buffer.set_text("");

    // Tags carry the appearance they were built for. Keeping them
    // across a change of scheme is what leaves a light window full of
    // dark diff tints, so the whole table is rebuilt when it no longer
    // matches.
    let marker = if dark { "scheme-dark" } else { "scheme-light" };
    if buffer.tag_table().lookup(marker).is_none() {
        let table = buffer.tag_table();
        let mut stale = Vec::new();
        table.foreach(|tag| stale.push(tag.clone()));
        for tag in stale {
            table.remove(&tag);
        }
        install_tags(&buffer, dark);
        table.add(&TextTag::builder().name(marker).build());
    }

    let mut offsets = Vec::with_capacity(file.rows.len());

    for (index, row) in file.rows.iter().enumerate() {
        let start_offset = buffer.end_iter().offset();
        offsets.push(start_offset);

        let gutter = format!(
            "{} {} {} ",
            number(row.old_line),
            number(row.new_line),
            row.kind.marker()
        );
        let mut end = buffer.end_iter();
        buffer.insert(&mut end, &gutter);

        // The gutter is dimmed as one run, before the code goes in, so
        // the syntax offsets below are relative to the code alone.
        let gutter_start = buffer.iter_at_offset(start_offset);
        let gutter_end = buffer.end_iter();
        buffer.apply_tag_by_name("gutter", &gutter_start, &gutter_end);

        let code_offset = buffer.end_iter().offset();
        let mut end = buffer.end_iter();
        buffer.insert(&mut end, &row.text);

        // Syntax spans are byte offsets into the row's text; GTK counts
        // characters, so they are converted rather than used directly —
        // otherwise any line with a non-ASCII character colours crooked.
        for span in &row.spans {
            let Some(prefix) = row.text.get(..span.start) else { continue };
            let Some(slice) = row.text.get(span.start..span.end) else { continue };
            let from = code_offset + prefix.chars().count() as i32;
            let to = from + slice.chars().count() as i32;
            let tag = format!("syn{}", span.style);
            if buffer.tag_table().lookup(&tag).is_some() {
                buffer.apply_tag_by_name(
                    &tag,
                    &buffer.iter_at_offset(from),
                    &buffer.iter_at_offset(to),
                );
            }
        }

        let mut end = buffer.end_iter();
        buffer.insert(&mut end, "\n");

        let tint = match row.kind {
            RowKind::Addition => Some("addition"),
            RowKind::Deletion => Some("deletion"),
            RowKind::HunkHeader => Some("header"),
            _ => None,
        };
        if let Some(tint) = tint {
            buffer.apply_tag_by_name(
                tint,
                &buffer.iter_at_offset(start_offset),
                &buffer.end_iter(),
            );
        }

        for annotation in annotations.iter().filter(|a| a.row == index) {
            insert_comment(&buffer, annotation.comment);
        }
    }

    Painted { offsets }
}
