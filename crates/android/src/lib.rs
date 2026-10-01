//! Drives bravebot for the Android app, over JNI.
//!
//! The same transport as `bravebot-rpc`, with the pipe replaced by two calls. The app hands a line
//! in with `nativeSend`, and every line that comes back (a response or an event) goes out
//! through the listener's `onLine`. The lines are the newline-delimited JSON the desktop app
//! speaks, unchanged, so the renderer's protocol mirror needs nothing new to run here. Everything
//! that decides anything lives in `bravebot-ui-bridge`, exactly as it does behind the pipe.
//!
//! Requests are dispatched on one thread of this library's own, which stands in for the loop
//! over stdin. A turn runs on a thread the bridge starts, and emits from there. Both attach to
//! the VM to call the listener, so the app must not assume `onLine` arrives on any particular
//! thread.
//!
//! **This is the only file in the crate that is `unsafe`, and only for the names the VM looks
//! up.** `#[unsafe(no_mangle)]` is what lets the runtime find `Java_…` symbols, and there is no
//! way to export one without it. Nothing else here reaches past the `jni` crate's safe API.

#![deny(unsafe_code)]

use bravebot_ui_bridge::bridge::Bridge;
use bravebot_ui_bridge::protocol::{Event, Request, Unreadable, response};
use jni::errors::LogErrorAndDefault;
use jni::objects::{JClass, JObject, JString, JValue};
use jni::refs::Global;
use jni::sys::{jboolean, jlong};
use jni::{EnvUnowned, JavaVM, jni_sig, jni_str};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// The bridges this process is running, by the handle the app was given.
///
/// A map rather than a pointer handed across as a `long`: a stale or forged handle then finds
/// nothing, instead of being dereferenced.
static RUNNING: Mutex<BTreeMap<jlong, Sender<String>>> = Mutex::new(BTreeMap::new());
static NEXT: AtomicI64 = AtomicI64::new(1);

/// Where lines go: one Kotlin object, called from whichever thread has a line.
struct Listener {
    vm: JavaVM,
    target: Global<JObject<'static>>,
}

impl Listener {
    /// Hand one line to the app, or drop it.
    ///
    /// Never fails, for the reason `Emitter::send` never fails: a listener that is gone is merely
    /// not drawing. A Java exception thrown by `onLine` comes back as an error and is dropped with
    /// the line, rather than left pending to fail the next JNI call this thread makes.
    fn line(&self, value: &serde_json::Value) {
        // Attached for the life of the thread, since the threads that emit (the dispatch thread
        // and the bridge's turn threads) emit many times. Each call gets a local frame of its own,
        // so the string made here does not outlive it.
        let _ = self
            .vm
            .attach_current_thread(|env| -> jni::errors::Result<()> {
                let text = env.new_string(value.to_string())?;
                env.call_method(
                    &*self.target,
                    jni_str!("onLine"),
                    jni_sig!("(Ljava/lang/String;)V"),
                    &[JValue::Object(&text)],
                )?;
                Ok(())
            });
    }
}

/// The loop `bravebot-rpc` runs over stdin, over a channel instead.
fn serve(listener: Arc<Listener>, settings: Option<PathBuf>, lines: Receiver<String>) {
    let emitter = Arc::clone(&listener);
    let mut bridge = Bridge::new(Box::new(move |event: Event| {
        emitter.line(&event.to_value());
    }))
    .with_settings(settings);

    bridge.ready();

    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        match Request::parse(&line) {
            Ok(request) => {
                let outcome = bridge.dispatch(&request);
                listener.line(&response(request.id, outcome));
            }
            Err(Unreadable::Answerable { id, failure }) => {
                listener.line(&response(id, Err(failure)));
            }
            // Nothing addressable to answer. The app wrote this line itself, so there is no
            // one further to tell.
            Err(Unreadable::Unanswerable { .. }) => {}
        }
    }

    // The app stopped this bridge. Anything still waiting on an answer is refused, which is what
    // dropping it does, as at EOF on the pipe.
    drop(bridge);
}

/// Start a bridge and return its handle, or 0 if it could not be started.
///
/// `settings` names a settings file, or is null for the default under `HOME`. The app sets `HOME`
/// before this is called, since an Android process is not given one.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_brave_bravebot_NativeBridge_nativeStart<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    listener: JObject<'local>,
    settings: JString<'local>,
) -> jlong {
    // First, for CRED-23: a credential is in this process's memory from the first turn.
    let _ = bravebot_agent::crash::disable_core_dumps();

    env.with_env(|env| -> jni::errors::Result<jlong> {
        let settings = if settings.is_null() {
            None
        } else {
            Some(PathBuf::from(settings.try_to_string(env)?))
        };
        let listener = Arc::new(Listener {
            vm: env.get_java_vm()?,
            target: env.new_global_ref(listener)?,
        });

        let (sender, receiver) = mpsc::channel();
        let started = std::thread::Builder::new()
            .name("bravebot-dispatch".into())
            .spawn(move || serve(listener, settings, receiver));
        if started.is_err() {
            return Ok(0);
        }

        let handle = NEXT.fetch_add(1, Ordering::Relaxed);
        let Ok(mut running) = RUNNING.lock() else {
            return Ok(0);
        };
        running.insert(handle, sender);
        Ok(handle)
    })
    .resolve::<LogErrorAndDefault>()
}

/// Hand one line to a bridge. False if there is no such bridge, or it has stopped.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_brave_bravebot_NativeBridge_nativeSend<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    handle: jlong,
    line: JString<'local>,
) -> jboolean {
    env.with_env(|env| -> jni::errors::Result<jboolean> {
        let line = line.try_to_string(env)?;
        let Ok(running) = RUNNING.lock() else {
            return Ok(false);
        };
        Ok(matches!(
            running.get(&handle).map(|sender| sender.send(line)),
            Some(Ok(()))
        ))
    })
    .resolve::<LogErrorAndDefault>()
}

/// Stop a bridge. Its dispatch thread ends once it has finished the line in hand.
///
/// Does not wait for that: the app calls this from its main thread, which must not block on a
/// request mid-flight.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_brave_bravebot_NativeBridge_nativeStop<'local>(
    _env: EnvUnowned<'local>,
    _class: JClass<'local>,
    handle: jlong,
) {
    if let Ok(mut running) = RUNNING.lock() {
        running.remove(&handle);
    }
}
