//! Painting a rendered file into a text view.
//!
//! The core decides what the rows are; this decides what they look like.
//! Nothing here works out which line a row stands for — that arrives
//! already settled in [`RenderedFile`], which is the whole point of the
//! core owning it.

use gtk::prelude::*;
use gtk::{TextBuffer, TextTag, TextView};

use prchum_core::render::{RenderedFile, RowKind};
use prchum_core::syntax;

/// Width of the two line-number columns, which keeps the marker column
/// and the code aligned down the file.
const NUMBER_WIDTH: usize = 5;

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
/// the row tints and the gutter.
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

/// Fills `view` with `file`, returning the character offset at which
/// each row starts so the caller can move the caret by row.
pub fn paint(view: &TextView, file: &RenderedFile, dark: bool) -> Vec<i32> {
    let buffer = view.buffer();
    buffer.set_text("");
    if buffer.tag_table().lookup("gutter").is_none() {
        install_tags(&buffer, dark);
    }

    let mut offsets = Vec::with_capacity(file.rows.len());

    for row in &file.rows {
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
    }

    offsets
}
