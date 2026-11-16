use std::{io, time::Duration};

#[cfg(feature = "event-stream")]
use super::sys::Waker;
use super::InternalEvent;

#[cfg(unix)]
pub(crate) mod unix;
#[cfg(windows)]
pub(crate) mod windows;

/// An interface for trying to read an `InternalEvent` within an optional `Duration`.
pub(crate) trait EventSource: Sync + Send {
    /// Tries to read an `InternalEvent` within the given duration.
    ///
    /// # Arguments
    ///
    /// * `timeout` - `None` block indefinitely until an event is available, `Some(duration)` blocks
    ///   for the given timeout
    ///
    /// Returns `Ok(None)` if there's no event available and timeout expires.
    fn try_read(&mut self, timeout: Option<Duration>) -> io::Result<Option<InternalEvent>>;

    /// Returns a `Waker` allowing to wake/force the `try_read` method to return `Ok(None)`.
    #[cfg(feature = "event-stream")]
    fn waker(&self) -> Waker;

    /// The mouse buttons this source last saw held, which its next mouse record is read against.
    #[cfg(windows)]
    fn mouse_buttons_pressed(&self) -> super::sys::windows::parse::MouseButtonsPressed {
        super::sys::windows::parse::MouseButtonsPressed::default()
    }

    /// Set the mouse buttons this source reads its next mouse record against.
    #[cfg(windows)]
    fn set_mouse_buttons_pressed(
        &mut self,
        _buttons: super::sys::windows::parse::MouseButtonsPressed,
    ) {
    }
}
