//! Platform-independent review core.
//!
//! Nothing in this crate may depend on a UI toolkit, an OS-specific API, or
//! perform any drawing. The core answers "what changed, and what do we know
//! about it?"; a platform shell answers "how does it look and feel?".

pub mod app;
pub mod config;
pub mod context;
pub mod diff;
pub mod editor;
pub mod exchange;
pub mod export;
pub mod history;
pub mod host;
pub mod location;
pub mod render;
pub mod review;
pub mod session;
pub mod source;
pub mod syntax;
pub mod util;
pub mod worktree;

pub use app::{App, Event, EventSender};
pub use config::Config;
pub use diff::{DiffLine, FileDiff, FileStatus, Hunk, LineKind, Side};
pub use location::{Location, RelocateResult};
pub use render::{render_file, render_split, RenderedFile, Row, RowKind, StyledSpan};
pub use review::{DraftComment, DraftReview, DraftState, ReviewEvent};
pub use session::Session;
