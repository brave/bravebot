//! Where events go, shared between the thread that dispatches and the thread that works.
//!
//! A turn runs off the dispatch thread so a slow model does not stall the interface, and
//! it reports as it goes. Both threads therefore emit, and both go through one handle so
//! their lines cannot interleave.
//!
//! Emitting cannot fail. That is not laziness: progress **announces**, where a write
//! **asks**, and the difference is consent. A listener that has gone away is merely not
//! drawing, and failing a turn because nobody was watching would let the display outrank
//! the work. Every error path here is therefore a silent drop — which is exactly the
//! wrong behaviour for [`crate::turn::BridgeConfirmer`], and why that is a separate type.

use crate::protocol::Event;
use std::sync::{Arc, Mutex};

/// Whatever a transport does with an event.
pub type Listener = Box<dyn FnMut(Event) + Send>;

/// A handle onto that, shared between the threads that emit.
#[derive(Clone)]
pub struct Emitter(Arc<Mutex<Output>>);

struct Output {
    sink: Listener,
    views: std::collections::HashMap<String, crate::view::View>,
}

impl Output {
    fn update(&mut self, session: &str, update: crate::view::Update) {
        (self.sink)(Event::new(
            "session.view.update",
            session,
            serde_json::json!(update),
        ));
    }

    fn send(&mut self, event: Event) {
        if let Some(session) = &event.session
            && let Some(view) = self.views.get_mut(session)
            && let Some(update) = view.event(&event)
        {
            self.update(session, update);
        }
        (self.sink)(event);
    }
}

impl Emitter {
    pub fn new(sink: Listener) -> Self {
        Self(Arc::new(Mutex::new(Output {
            sink,
            views: Default::default(),
        })))
    }

    /// Announce something. Never fails, by design.
    ///
    /// A poisoned lock means another thread panicked mid-emit. The event is dropped
    /// rather than propagating that panic into a turn, since a turn that is working is
    /// worth more than a line about it.
    pub fn send(&self, event: Event) {
        if let Ok(mut sink) = self.0.lock() {
            sink.send(event);
        }
    }
}

impl Emitter {
    pub fn has_view(&self, session: &str) -> bool {
        self.0
            .lock()
            .is_ok_and(|output| output.views.contains_key(session))
    }

    /// Install once, before the first turn. The initial event shares the update lock.
    pub fn start_view(&self, session: &str, trusted: bool) -> Result<(), crate::protocol::Failure> {
        let mut output = self
            .0
            .lock()
            .map_err(|_| crate::protocol::Failure::bad_request("event stream unavailable"))?;
        if output.views.contains_key(session) {
            return Err(crate::protocol::Failure::bad_request(
                "view already started",
            ));
        }
        let view = crate::view::View::new(trusted);
        (output.sink)(Event::new(
            "session.view.initial",
            session,
            serde_json::json!(view.initial()),
        ));
        output.views.insert(session.to_string(), view);
        Ok(())
    }

    pub fn view_started(&self, session: &str, turn: u64, target: u64, prompt: serde_json::Value) {
        if let Ok(mut output) = self.0.lock()
            && let Some(view) = output.views.get_mut(session)
        {
            let update = view.started(turn, target, prompt);
            output.update(session, update);
        }
    }

    pub fn view_trusted(&self, session: &str) {
        if let Ok(mut output) = self.0.lock()
            && let Some(view) = output.views.get_mut(session)
            && let Some(update) = view.trusted()
        {
            output.update(session, update);
        }
    }

    pub fn view_answered(&self, session: &str, request: u64) {
        if let Ok(mut output) = self.0.lock()
            && let Some(view) = output.views.get_mut(session)
            && let Some(update) = view.answered(request)
        {
            output.update(session, update);
        }
    }

    pub fn detach_view(&self, session: &str) {
        if let Ok(mut output) = self.0.lock()
            && let Some(mut view) = output.views.remove(session)
        {
            output.update(session, view.detach());
        }
    }

    /// Publish completion and make the next turn eligible under the same output lock.
    pub fn finish(&self, event: Event, finished: &std::sync::atomic::AtomicBool) {
        if let Ok(mut output) = self.0.lock() {
            finished.store(true, std::sync::atomic::Ordering::Release);
            output.send(event);
        } else {
            finished.store(true, std::sync::atomic::Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    /// The callback can observe completion before dispatch receives another request.
    /// That request must see eligibility and cannot publish ahead of the terminal update.
    #[test]
    fn completion_is_eligible_before_publication_and_precedes_the_next_turn() {
        let finished = Arc::new(AtomicBool::new(false));
        let observed = finished.clone();
        let (seen, received) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let emitter = Emitter::new(Box::new(move |event| {
            if event.name == "session.view.update" {
                let completed = event.data["status"] == "completed";
                seen.send((event, observed.load(Ordering::Acquire)))
                    .unwrap();
                if completed {
                    wait.recv_timeout(Duration::from_secs(5)).unwrap();
                }
            }
        }));
        emitter.start_view("s1", true).unwrap();
        let worker = emitter.clone();
        let ending = std::thread::spawn(move || {
            worker.finish(Event::new("turn.done", "s1", json!({"turn": 1})), &finished)
        });
        let (terminal, eligible) = received.recv_timeout(Duration::from_secs(5)).unwrap();
        let next = emitter.clone();
        let starting =
            std::thread::spawn(move || next.view_started("s1", 2, 2, json!({"text": "next"})));
        release.send(()).unwrap();
        ending.join().unwrap();
        starting.join().unwrap();
        assert!(
            eligible,
            "a client observing completion must be able to send"
        );
        let (started, _) = received.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(terminal.data["sequence"], 1);
        assert_eq!(terminal.data["status"], "completed");
        assert_eq!(started.data["sequence"], 2);
        assert_eq!(started.data["status"], "running");
    }
}
