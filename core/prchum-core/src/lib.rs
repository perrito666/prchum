//! Platform-independent review core.
//!
//! Nothing in this crate may depend on a UI toolkit, an OS-specific API, or
//! perform any drawing. The core answers "what changed, and what do we know
//! about it?"; a platform shell answers "how does it look and feel?".

pub mod app;
pub mod diff;
pub mod session;

pub use app::{App, Event, EventSender};
pub use diff::{DiffLine, FileDiff, FileStatus, Hunk, LineKind, Side};
pub use session::Session;
