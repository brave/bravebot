package com.brave.bravebot

/**
 * The JNI surface of `crates/android`.
 *
 * Three calls: start a bridge, hand it a line, stop it. Every line that comes back (a response or
 * an event, in the bridge's own newline-delimited JSON) arrives at the listener, on whichever
 * native thread produced it.
 */
object NativeBridge {
    fun interface Listener {
        fun onLine(line: String)
    }

    init {
        System.loadLibrary("bravebot_android")
    }

    /** A handle for [nativeSend] and [nativeStop], or 0 if the bridge could not start. */
    @JvmStatic external fun nativeStart(listener: Listener, settings: String?): Long

    /** False if there is no such bridge or it has stopped. */
    @JvmStatic external fun nativeSend(handle: Long, line: String): Boolean

    /** Returns at once; anything waiting on an answer is refused as the bridge winds down. */
    @JvmStatic external fun nativeStop(handle: Long)
}
