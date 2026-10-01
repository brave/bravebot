package com.brave.bravebot

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.content.res.Configuration
import android.graphics.Color
import android.os.Bundle
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.widget.FrameLayout
import android.widget.TextView
import androidx.webkit.WebViewAssetLoader
import androidx.webkit.WebViewClientCompat
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature

/**
 * One window: the desktop renderer, in a WebView.
 *
 * The page is served from the app's assets under an https origin of its own, which is what keeps
 * its storage between launches and what the host channel is registered for. It may not navigate
 * anywhere else; a link out opens in the browser instead.
 */
class MainActivity : Activity() {
    private var webView: WebView? = null
    private var frame: FrameLayout? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val app = application as BravebotApp

        if (!WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
            setContentView(TextView(this).apply {
                text = "Brave Bot needs a newer Android System WebView. Update it from the Play Store."
            })
            return
        }

        if (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0) {
            WebView.setWebContentsDebuggingEnabled(true)
        }

        val assets = WebViewAssetLoader.Builder()
            .addPathHandler("/assets/", WebViewAssetLoader.AssetsPathHandler(this))
            .build()

        val view = WebView(this)
        view.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            allowFileAccess = false
            allowContentAccess = false
            textZoom = textZoomFor(resources.configuration)
        }
        view.webViewClient = object : WebViewClientCompat() {
            override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? =
                assets.shouldInterceptRequest(request.url)

            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                if (request.url.host == ORIGIN_HOST) return false
                if (request.url.scheme == "https" || request.url.scheme == "http") {
                    try {
                        startActivity(Intent(Intent.ACTION_VIEW, request.url))
                    } catch (_: ActivityNotFoundException) {
                        // Nothing to open it with. The page stays where it was.
                    }
                }
                return true
            }
        }
        WebViewCompat.addWebMessageListener(view, "BravebotHost", setOf(ORIGIN), Host(app.agent, app.workspace, ::paintBars))

        // Edge to edge is the default from Android 15, so the page is kept clear of the status bar,
        // the navigation bar and the keyboard rather than drawn under them. The padding goes on a
        // frame around the WebView, since a WebView lays its page out ignoring its own padding.
        val frame = FrameLayout(this)
        frame.addView(view, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
        frame.setOnApplyWindowInsetsListener { v, insets ->
            val bars = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.ime())
            v.setPadding(bars.left, bars.top, bars.right, bars.bottom)
            WindowInsets.CONSUMED
        }

        setContentView(frame)
        this.frame = frame
        webView = view
        view.loadUrl("$ORIGIN/assets/renderer/index.html")
    }

    /**
     * Back closes whatever the page has open before it leaves the app.
     *
     * The page keeps a stack of what is open (a dialog, a menu, the drawer) and closes the newest
     * when asked; only a press it had nothing to close for goes on to the system. The agent
     * belongs to the process, so leaving the window does not end a turn.
     */
    @Deprecated("Still delivered to apps that have not opted into predictive back")
    override fun onBackPressed() {
        val view = webView
        if (view == null) {
            @Suppress("DEPRECATION")
            super.onBackPressed()
            return
        }
        view.evaluateJavascript(BACK_SCRIPT) { handled ->
            @Suppress("DEPRECATION")
            if (handled != "true") super.onBackPressed()
        }
    }

    /**
     * The system bars sit over the frame around the page, so the frame takes the page's colour and
     * the bars' icons go dark on a light page and light on a dark one.
     */
    private fun paintBars(color: Int) {
        frame?.setBackgroundColor(color)
        window.decorView.setBackgroundColor(color)
        val light = Color.luminance(color) > 0.5f
        val flags = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or
            WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
        window.insetsController?.setSystemBarsAppearance(if (light) flags else 0, flags)
    }

    /** The system's font size, which a WebView does not follow by itself. */
    private fun textZoomFor(configuration: Configuration): Int = (configuration.fontScale * 100).toInt()

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        webView?.settings?.textZoom = textZoomFor(newConfig)
    }

    override fun onDestroy() {
        webView?.destroy()
        webView = null
        super.onDestroy()
    }

    private companion object {
        const val ORIGIN_HOST = WebViewAssetLoader.DEFAULT_DOMAIN
        const val ORIGIN = "https://$ORIGIN_HOST"
        const val BACK_SCRIPT = "window.bravebotBack?.() === true"
    }
}
