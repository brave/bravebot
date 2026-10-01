package com.brave.bravebot

import android.os.Handler
import android.os.Looper
import androidx.annotation.MainThread
import org.json.JSONObject

/**
 * The bridge, seen from the main thread.
 *
 * Lines from the native side are moved onto the main thread here, because the page they go to may
 * only be written from there. Until a page is listening they are held, so `agent.ready`, which the
 * bridge announces before anything has asked, is not lost to a WebView still loading.
 */
class Agent {
    private val main = Handler(Looper.getMainLooper())
    private var handle = 0L
    private var sink: ((String) -> Unit)? = null
    private val backlog = ArrayDeque<String>()

    fun start() {
        handle = NativeBridge.nativeStart({ line -> main.post { emit(line) } }, null)
    }

    /** Where lines go from now on. Replaces whatever page was listening before. */
    @MainThread
    fun attach(to: (String) -> Unit) {
        sink = to
        while (backlog.isNotEmpty()) to(backlog.removeFirst())
    }

    /** Hand the bridge one request line. False if it is not running. */
    fun send(line: String): Boolean = handle != 0L && NativeBridge.nativeSend(handle, line)

    /**
     * List models on a bridge of its own, as the desktop does.
     *
     * Discovery asks every configured gateway over the network, and the main bridge answers one
     * request at a time, so a slow gateway listed there would hold up every approval behind it. The
     * answer is the discovery bridge's, with its id rewritten to the one the page is waiting on.
     */
    fun listModels(id: Long, directory: String, reply: (String) -> Unit) {
        var discovery = 0L
        discovery = NativeBridge.nativeStart({ line ->
            val message = runCatching { JSONObject(line) }.getOrNull()
            if (message != null && message.has("id")) {
                NativeBridge.nativeStop(discovery)
                message.put("id", id)
                main.post { reply(message.toString()) }
            }
        }, null)
        val request = JSONObject()
            .put("id", 1)
            .put("method", "models.list")
            .put("params", JSONObject().put("directory", directory))
        if (discovery == 0L || !NativeBridge.nativeSend(discovery, request.toString())) {
            main.post { reply(Host.failure(id, "no_bridge", "the agent is not running").toString()) }
        }
    }

    @MainThread
    private fun emit(line: String) {
        val to = sink
        if (to != null) {
            to(line)
            return
        }
        // Bounded: a page that never arrives must not grow this without limit.
        if (backlog.size >= BACKLOG_MAX) backlog.removeFirst()
        backlog.addLast(line)
    }

    private companion object {
        const val BACKLOG_MAX = 1000
    }
}
