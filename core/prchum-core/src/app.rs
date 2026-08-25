//! The core's root object and its one-way event channel.
//!
//! Asynchronous work (forge fetches, file content, submissions) reports back
//! through a single channel drained by one dedicated dispatch thread, so a
//! shell registers exactly one callback and never sees two events at once.

use std::sync::mpsc;
use std::thread::JoinHandle;

/// An event delivered from the core to the shell.
#[derive(Clone, Debug)]
pub enum Event {
    /// Reply to a ping; proves the async round trip end to end.
    Pong { seq: u64 },
}

/// Cloneable handle for core subsystems to emit events with.
#[derive(Clone)]
pub struct EventSender {
    tx: mpsc::Sender<Event>,
}

impl EventSender {
    pub fn send(&self, event: Event) {
        // A closed channel means the app is shutting down; drop silently.
        let _ = self.tx.send(event);
    }
}

/// Root core instance. Owns the dispatch thread; dropping the app closes the
/// channel and joins the thread, so no callback runs after teardown.
pub struct App {
    tx: Option<mpsc::Sender<Event>>,
    dispatcher: Option<JoinHandle<()>>,
}

impl App {
    /// Creates an app whose events are delivered, one at a time, to
    /// `callback` on a core-owned thread.
    pub fn new(callback: impl Fn(Event) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<Event>();
        let dispatcher = std::thread::Builder::new()
            .name("prchum-events".to_string())
            .spawn(move || {
                while let Ok(event) = rx.recv() {
                    callback(event);
                }
            })
            .expect("spawn event dispatcher");
        Self {
            tx: Some(tx),
            dispatcher: Some(dispatcher),
        }
    }

    pub fn sender(&self) -> EventSender {
        EventSender {
            tx: self.tx.clone().expect("sender before drop"),
        }
    }

    /// Asks the core to answer with `Event::Pong` from a worker thread.
    pub fn ping(&self, seq: u64) {
        let sender = self.sender();
        std::thread::spawn(move || sender.send(Event::Pong { seq }));
    }
}

impl Drop for App {
    fn drop(&mut self) {
        // Close the channel first so the dispatcher's recv() ends, then wait
        // for it: after drop returns, the callback is guaranteed quiescent.
        self.tx.take();
        if let Some(handle) = self.dispatcher.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn pong_round_trip() {
        let (probe_tx, probe_rx) = mpsc::channel();
        let app = App::new(move |event| {
            let Event::Pong { seq } = event;
            probe_tx.send(seq).unwrap();
        });
        app.ping(7);
        assert_eq!(probe_rx.recv_timeout(Duration::from_secs(5)).unwrap(), 7);
    }

    #[test]
    fn drop_joins_quietly() {
        let app = App::new(|_| {});
        app.ping(1);
        drop(app); // must not hang or panic
    }
}
