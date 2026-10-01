package com.brave.bravebot

import android.net.Uri
import android.webkit.WebView
import androidx.webkit.JavaScriptReplyProxy
import androidx.webkit.WebMessageCompat
import androidx.webkit.WebViewCompat
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import java.io.File

/**
 * The other end of `window.bravebot`: what the main process is on the desktop.
 *
 * The page is not trusted to decide what the agent may be asked. Every request is checked against
 * the same list of methods `ui/src/main/index.ts` allows, and `turn.send` and `manifest.run` lose
 * the same fields there, for the same reason: a page that could name a file to a turn could have
 * the planner read any file the app can. Keep the two lists in step.
 *
 * Only registered for the app's own asset origin, and answers only the main frame.
 */
class Host(
    private val agent: Agent,
    private val workspace: File,
    /** The colour the page is painted in, for the system bars around it. */
    private val onChrome: (Int) -> Unit,
) : WebViewCompat.WebMessageListener {
    private var page: JavaScriptReplyProxy? = null

    override fun onPostMessage(
        view: WebView,
        message: WebMessageCompat,
        sourceOrigin: Uri,
        isMainFrame: Boolean,
        replyProxy: JavaScriptReplyProxy,
    ) {
        if (!isMainFrame) return
        if (page !== replyProxy) {
            page = replyProxy
            agent.attach { line -> replyProxy.postMessage(line) }
        }
        val body = try {
            JSONObject(message.data ?: return)
        } catch (_: JSONException) {
            return
        }
        val id = (body.opt("id") as? Number)?.toLong() ?: return
        when {
            body.has("local") -> replyProxy.postMessage(local(id, body.optString("local"), body.optJSONArray("args")).toString())
            body.has("method") -> request(id, body.optString("method"), body.optJSONObject("params") ?: JSONObject(), replyProxy)
        }
    }

    private fun request(id: Long, method: String, params: JSONObject, page: JavaScriptReplyProxy) {
        if (method !in ALLOWED) {
            page.postMessage(failure(id, "bad_request", "not a permitted method: $method").toString())
            return
        }
        if (method == "models.list") {
            agent.listModels(id, workspace.absolutePath) { line -> page.postMessage(line) }
            return
        }
        val line = JSONObject().put("id", id).put("method", method).put("params", sanitised(method, params))
        if (!agent.send(line.toString())) {
            page.postMessage(failure(id, "no_bridge", "the agent is not running").toString())
        }
    }

    /** Calls that the app answers itself rather than the agent. */
    private fun local(id: Long, name: String, args: JSONArray?): JSONObject = when (name) {
        // One project for now, so choosing a directory and the recents both name it.
        "directory.choose" -> answer(id, workspace.absolutePath)
        "recents.read" -> answer(id, JSONArray().put(workspace.absolutePath))
        "chrome" -> {
            val color = cssColor(args?.optString(0).orEmpty())
            if (color != null) onChrome(color)
            answer(id, color != null)
        }
        else -> failure(id, "bad_request", "not a permitted call: $name")
    }

    companion object {
        /** `ALLOWED` in `ui/src/main/index.ts`. */
        val ALLOWED = setOf(
            "agent.info", "models.list",
            "session.list", "session.open", "session.new", "session.fork", "session.close",
            "turn.send", "turn.cancel",
            "confirm.reply", "run.reply", "output.reply", "vouch.reply", "vet.reply",
            "fetch.reply", "server.reply",
            "manifest.run", "manifest.read", "manifest.reply",
            "exposure.reply", "ask.reply", "trust.reply",
            "permissions.list", "permissions.revoke",
            "watches.list", "watches.add", "watches.stop",
            "settings.inspect", "hooks.inspect", "doctor",
        )

        /** `sanitised` in `ui/src/main/index.ts`. Attachments are not offered here yet, so none survive. */
        fun sanitised(method: String, params: JSONObject): JSONObject = when (method) {
            "manifest.run" -> JSONObject()
                .put("session", params.opt("session"))
                .put("task", params.opt("task"))
                .put("model", params.opt("model"))
            "turn.send" -> JSONObject(params.toString()).apply {
                remove("files")
                remove("dropped")
                remove("recall")
                remove("attachments")
                put("files", JSONArray())
            }
            else -> params
        }

        /** `rgb(r, g, b)` or `rgba(r, g, b, a)` as computed style writes it, opaque; null for anything else. */
        fun cssColor(text: String): Int? {
            val parts = Regex("""^rgba?\((\d+),\s*(\d+),\s*(\d+)""").find(text)?.groupValues ?: return null
            val (r, g, b) = parts.drop(1).map { it.toInt().coerceIn(0, 255) }
            return (0xFF shl 24) or (r shl 16) or (g shl 8) or b
        }

        fun answer(id: Long, ok: Any): JSONObject = JSONObject().put("id", id).put("ok", ok)

        fun failure(id: Long, code: String, message: String): JSONObject =
            JSONObject().put("id", id).put("error", JSONObject().put("code", code).put("message", message))
    }
}
