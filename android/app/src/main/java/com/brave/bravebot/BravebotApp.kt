package com.brave.bravebot

import android.app.Application
import android.system.Os
import java.io.File

/**
 * Process-wide state: where the agent keeps things, and the one bridge driving it.
 *
 * The bridge belongs to the process rather than to an activity, so a turn outlives the window
 * that started it being rebuilt.
 */
class BravebotApp : Application() {
    lateinit var agent: Agent
        private set

    /** The one project this build offers: a folder in the app's own storage. */
    lateinit var workspace: File
        private set

    override fun onCreate() {
        super.onCreate()
        // An Android process is given no HOME, and without one the agent's state directory
        // (`~/.bravebot`) resolves to nothing and saving quietly stops. Nor is it given a temporary
        // directory it can write: the default, /data/local/tmp, belongs to the shell user. All three
        // point into this app's private storage, before the library reads any of them.
        Os.setenv("HOME", filesDir.absolutePath, true)
        Os.setenv("XDG_CACHE_HOME", cacheDir.absolutePath, true)
        Os.setenv("TMPDIR", cacheDir.absolutePath, true)

        workspace = File(filesDir, "workspace").apply { mkdirs() }
        agent = Agent().also { it.start() }
    }
}
