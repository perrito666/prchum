//! Building the editor invocation for "open this file locally".
//!
//! The editor is a template with `{path}`, `{line}`, and `{dir}`
//! placeholders — the same shape as the Forgejo transport template, so
//! there is one thing to learn. Two kinds are supported and told apart by
//! the template itself:
//!
//! * a **URL** (it has a scheme): `textchum://open?path={path}&line={line}`
//!   — the default, since textchum opens files that way;
//! * a **command**: `code -g {path}:{line}`, `nvim +{line} {path}`, …
//!
//! The core only builds the invocation; performing it belongs to the
//! shell, which knows how its platform opens URLs and spawns processes.

/// What the shell should do to open the file.
#[derive(Clone, Debug, PartialEq)]
pub enum Invocation {
    /// Hand this URL to the platform's opener.
    Url(String),
    /// Run this program with these arguments.
    Command { program: String, args: Vec<String> },
}

/// The default: textchum, which registers `textchum://open`.
pub const DEFAULT_EDITOR_COMMAND: &str = "textchum://open?path={path}&line={line}";

/// Builds the invocation for `path` at `line` (0 = no particular line).
/// An empty template means the built-in default.
pub fn invocation(template: &str, path: &str, line: u32, dir: &str) -> Invocation {
    let template = if template.trim().is_empty() {
        DEFAULT_EDITOR_COMMAND
    } else {
        template.trim()
    };

    if is_url(template) {
        // Values inside a URL are percent-encoded; `{line}` is a number
        // either way.
        let filled = template
            .replace("{path}", &percent_encode(path))
            .replace("{dir}", &percent_encode(dir))
            .replace("{line}", &line.max(1).to_string());
        return Invocation::Url(filled);
    }

    let mut words = Vec::new();
    for word in template.split_whitespace() {
        words.push(
            word.replace("{path}", path)
                .replace("{dir}", dir)
                .replace("{line}", &line.max(1).to_string()),
        );
    }
    // A template that never mentions the file still has to receive it.
    if !template.contains("{path}") {
        words.push(path.to_string());
    }
    let program = if words.is_empty() {
        String::new()
    } else {
        words.remove(0)
    };
    Invocation::Command {
        program,
        args: words,
    }
}

/// A template is a URL when it opens with `scheme://`.
fn is_url(template: &str) -> bool {
    match template.split_once("://") {
        Some((scheme, _)) => {
            !scheme.is_empty()
                && !scheme.contains(char::is_whitespace)
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
        }
        None => false,
    }
}

/// Percent-encoding for a URL query value: everything outside the
/// unreserved set is escaped, so paths with spaces, `&`, or `#` survive.
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_a_textchum_url() {
        let Invocation::Url(url) = invocation("", "/tmp/a b.rs", 42, "/tmp") else {
            panic!("the default must be a URL");
        };
        assert!(url.starts_with("textchum://open?path="));
        assert!(url.contains("%2Ftmp%2Fa%20b.rs"), "{url}");
        assert!(url.ends_with("&line=42"));
    }

    #[test]
    fn commands_split_and_substitute() {
        assert_eq!(
            invocation("code -g {path}:{line}", "/tmp/a.rs", 7, "/tmp"),
            Invocation::Command {
                program: "code".into(),
                args: vec!["-g".into(), "/tmp/a.rs:7".into()],
            }
        );
        assert_eq!(
            invocation("nvim +{line} {path}", "/tmp/a.rs", 7, "/tmp"),
            Invocation::Command {
                program: "nvim".into(),
                args: vec!["+7".into(), "/tmp/a.rs".into()],
            }
        );
    }

    #[test]
    fn a_template_without_the_path_still_gets_it() {
        assert_eq!(
            invocation("open -a Xcode", "/tmp/a.rs", 0, "/tmp"),
            Invocation::Command {
                program: "open".into(),
                args: vec!["-a".into(), "Xcode".into(), "/tmp/a.rs".into()],
            }
        );
    }

    #[test]
    fn line_zero_means_the_first_line() {
        let Invocation::Url(url) = invocation("", "/a", 0, "/") else {
            panic!("url expected");
        };
        assert!(url.ends_with("&line=1"), "{url}");
    }

    #[test]
    fn url_detection() {
        assert!(is_url("textchum://open?path={path}"));
        assert!(is_url("vscode://file/{path}:{line}"));
        assert!(!is_url("code -g {path}"));
        assert!(!is_url("/usr/local/bin/edit {path}"));
    }
}
