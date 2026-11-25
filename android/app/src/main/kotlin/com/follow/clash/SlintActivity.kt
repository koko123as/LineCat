package com.follow.clash

import android.Manifest
import android.app.NativeActivity
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.net.VpnService
import android.os.Build
import android.os.Bundle
import android.os.Process
import android.view.SurfaceView
import android.view.ViewGroup
import android.view.ViewTreeObserver
import androidx.core.view.WindowCompat

/**
 * The launcher activity: the Slint UI renders into this NativeActivity from
 * libseasoncat_ui.so (meta-data android.app.lib_name). This shell only does what the Rust
 * side must not: the VPN consent dialog and the permission prompts, plus the socket bridge
 * in [UiBridge].
 */
class SlintActivity : NativeActivity() {
    private var vpnCallback: ((Boolean) -> Unit)? = null
    private var documentCallback: ((android.net.Uri?) -> Unit)? = null
    private var permissionCallback: ((Boolean) -> Unit)? = null
    private val layoutListener = ViewTreeObserver.OnGlobalLayoutListener { reportEdges() }

    override fun onCreate(savedInstanceState: Bundle?) {
        val crash = java.io.File(filesDir, "crash.txt")
        if (crash.exists()) {
            // Diagnostic: show why the last start failed (in its own process), so it can be reported without adb.
            val text = runCatching { crash.readText() }.getOrDefault("")
            crash.delete()
            startActivity(Intent(this, CrashActivity::class.java).putExtra("text", text))
            super.onCreate(savedInstanceState)
            finish()
            return
        }
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { t, e ->
            runCatching { crash.writeText("java crash in ${t.name}:\n" + android.util.Log.getStackTraceString(e)) }
            previous?.uncaughtException(t, e)
        }
        // The bridge listens before the native thread starts and connects to it.
        UiBridge.start(applicationContext)
        UiBridge.activity = this
        SystemFont.export(applicationContext)
        super.onCreate(savedInstanceState)
        // Draw into a SurfaceView sized by layout, as GameActivity does, instead of the window
        // surface NativeActivity takes: in Flyme's small window that surface keeps the full
        // screen's size and is scaled down into the window, shrinking the whole phone layout.
        WindowCompat.setDecorFitsSystemWindows(window, false)
        window.takeSurface(null)
        val surface = SurfaceView(this)
        surface.holder.addCallback(this)
        addContentView(surface, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        window.decorView.viewTreeObserver.addOnGlobalLayoutListener(layoutListener)
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        window.decorView.post { reportEdges() }
    }

    @Suppress("OVERRIDE_DEPRECATION", "DEPRECATION")
    override fun onMultiWindowModeChanged(isInMultiWindowMode: Boolean) {
        super.onMultiWindowModeChanged(isInMultiWindowMode)
        window.decorView.post { reportEdges() }
    }

    private fun reportEdges() {
        WindowEdges.of(window.decorView)?.let(UiBridge::reportEdges)
    }

    override fun onResume() {
        super.onResume()
        UiBridge.activity = this
        window.decorView.post { reportEdges() }
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) window.decorView.post { reportEdges() }
    }

    override fun onDestroy() {
        if (UiBridge.activity === this) UiBridge.activity = null
        window.decorView.viewTreeObserver.let { if (it.isAlive) it.removeOnGlobalLayoutListener(layoutListener) }
        vpnCallback?.invoke(false)
        vpnCallback = null
        documentCallback?.invoke(null)
        documentCallback = null
        permissionCallback?.invoke(false)
        permissionCallback = null
        val exit = isFinishing && UiBridge.isStopped
        super.onDestroy()
        // Nothing runs once the UI is closed and the VPN is off, so the process goes with it
        // instead of keeping the Core and the renderer resident.
        if (exit) Process.killProcess(Process.myPid())
    }

    fun prepareVpn(callback: (Boolean) -> Unit) {
        val intent = VpnService.prepare(this)
        if (intent == null) {
            callback(true)
            return
        }
        vpnCallback?.invoke(false)
        vpnCallback = callback
        runCatching { startActivityForResult(intent, VPN_REQUEST) }
            .onFailure {
                vpnCallback = null
                callback(false)
            }
    }

    /** A document picker (open or create); [callback] gets the chosen uri, or null. */
    fun startForResult(intent: Intent, callback: (android.net.Uri?) -> Unit) {
        documentCallback?.invoke(null)
        documentCallback = callback
        runCatching { startActivityForResult(intent, DOCUMENT_REQUEST) }
            .onFailure {
                documentCallback = null
                callback(null)
            }
    }

    fun requestNotificationPermission() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) return
        val preferences = getSharedPreferences(PREFERENCES, MODE_PRIVATE)
        if (preferences.getBoolean(NOTIFICATION_ASKED, false)) return
        preferences.edit().putBoolean(NOTIFICATION_ASKED, true).apply()
        requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), NOTIFICATION_REQUEST)
    }

    /** A runtime permission prompt; [callback] gets whether it was granted. */
    fun requestPermission(permission: String, callback: (Boolean) -> Unit) {
        permissionCallback?.invoke(false)
        permissionCallback = callback
        runCatching { requestPermissions(arrayOf(permission), PERMISSION_REQUEST) }
            .onFailure {
                permissionCallback = null
                callback(false)
            }
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode != PERMISSION_REQUEST) return
        val callback = permissionCallback
        permissionCallback = null
        callback?.invoke(grantResults.isNotEmpty() && grantResults.all { it == PackageManager.PERMISSION_GRANTED })
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        if (requestCode == VPN_REQUEST) {
            val callback = vpnCallback
            vpnCallback = null
            callback?.invoke(resultCode == RESULT_OK)
            return
        }
        if (requestCode == DOCUMENT_REQUEST) {
            val callback = documentCallback
            documentCallback = null
            callback?.invoke(if (resultCode == RESULT_OK) data?.data else null)
            return
        }
        super.onActivityResult(requestCode, resultCode, data)
    }

    private companion object {
        const val VPN_REQUEST = 0x5643
        const val NOTIFICATION_REQUEST = 0x4e54
        const val DOCUMENT_REQUEST = 0x4443
        const val PERMISSION_REQUEST = 0x5052
        const val PREFERENCES = "seasoncat_shell"
        const val NOTIFICATION_ASKED = "notification_asked"
    }
}
