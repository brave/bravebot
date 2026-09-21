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
pub struct Emitter(Arc<Mutex<Listener>>);

impl Emitter {
    pub fn new(sink: Listener) -> Self {
        Self(Arc::new(Mutex::new(sink)))
    }

    /// Announce something. Never fails, by design.
    ///
    /// A poisoned lock means another thread panicked mid-emit. The event is dropped
    /// rather than propagating that panic into a turn, since a turn that is working is
    /// worth more than a line about it.
    pub fn send(&self, event: Event) {
        if let Ok(mut sink) = self.0.lock() {
            sink(event);
        }
    }
}
