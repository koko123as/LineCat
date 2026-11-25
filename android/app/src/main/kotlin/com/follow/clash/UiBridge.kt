package com.follow.clash

import android.content.Context
import android.graphics.Bitmap
import android.net.LocalServerSocket
import android.net.LocalSocket
import android.os.Build
import android.os.Process
import androidx.core.graphics.drawable.toBitmap
import com.follow.clash.common.GlobalState
import com.follow.clash.core.Core
import com.follow.clash.core.NativeCore
import com.follow.clash.models.SharedState
import com.follow.clash.packages.PackageResolver
import com.follow.clash.plugins.DefaultNetworkWatcher
import com.follow.clash.service.NetworkObserver
import com.follow.clash.service.models.VpnOptions
import com.google.gson.Gson
import com.google.gson.JsonArray
import com.google.gson.JsonObject
import com.google.gson.JsonParser
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.util.concurrent.CopyOnWriteArraySet
import java.util.concurrent.LinkedBlockingQueue
import kotlin.coroutines.resume

/**
 * The local socket the Slint UI (libseasoncat_ui.so) talks to, so the Rust side makes no JNI
 * calls of its own. The socket lives in the abstract namespace as `<applicationId>.ui` and takes
 * only peers running as this app's uid.
 *
 * A frame is a u32 big-endian payload length, one channel byte and the payload. Channel 0 carries
 * the Core's JSON-RPC frames, forwarded to and from a Core session opened for the connection, as
 * ServicePlugin forwards Flutter's. Channel 1 carries shell commands and pushed state events.
 */
object UiBridge {
    private const val CH_CORE = 0
    private const val CH_SHELL = 1
    private const val MAX_FRAME = 64 shl 20
    private val GEO_ASSETS = listOf("GEOIP.metadb", "GEOSITE.dat")

    private val gson = Gson()
    private val packageResolver by lazy {
        PackageResolver(GlobalState.application.packageManager, GlobalState.application.packageName)
    }
    private val connections = CopyOnWriteArraySet<Connection>()

    @Volatile
    private var started = false

    @Volatile
    var activity: SlintActivity? = null

    @Synchronized
    fun start(context: Context) {
        if (started) return
        started = true
        runCatching { Core.load() }.onFailure { GlobalState.log("Unable to load the Core: $it") }
        NetworkObserver.start()
        context.getSystemService(android.net.ConnectivityManager::class.java)?.let { connectivity ->
            DefaultNetworkWatcher(connectivity) { state ->
                val wifi = state["wifi"] == true
                onWifi = wifi
                broadcast(JsonObject().apply {
                    addProperty("ev", "network")
                    addProperty("wifi", wifi)
                })
            }.start()
        }
        val server = runCatching { LocalServerSocket("${context.packageName}.ui") }
            .onFailure { GlobalState.log("Unable to open the UI bridge: $it") }
            .getOrNull() ?: return
        Thread({ accept(server) }, "ui-bridge-accept").apply { isDaemon = true }.start()
        GlobalState.launch {
            ServiceState.runState.collect { state ->
                broadcast(JsonObject().apply {
                    addProperty("ev", "state")
                    addProperty("state", state.name)
                })
            }
        }
    }

    @Volatile
    private var onWifi = false

    /** The activity came back to the front: permissions may have changed in the settings. */
    fun resumed() {
        broadcast(JsonObject().apply { addProperty("ev", "resume") })
    }

    val isStopped: Boolean
        get() = ServiceState.runState.value == RunState.STOPPED

    private fun accept(server: LocalServerSocket) {
        while (true) {
            val socket = try {
                server.accept()
            } catch (error: IOException) {
                GlobalState.log("UI bridge stopped accepting: $error")
                return
            }
            val uid = runCatching { socket.peerCredentials.uid }.getOrDefault(-1)
            if (uid != Process.myUid()) {
                GlobalState.log("UI bridge refused uid $uid")
                runCatching { socket.close() }
                continue
            }
            Connection(socket).start()
        }
    }

    @Volatile
    private var edges: JsonObject? = null

    fun reportEdges(message: JsonObject) {
        if (message == edges) return
        edges = message
        broadcast(message)
    }

    private fun broadcast(message: JsonObject) {
        connections.forEach { it.send(CH_SHELL, message) }
    }

    private class Connection(private val socket: LocalSocket) {
        private val output = LinkedBlockingQueue<Pair<Int, ByteArray>>()
        private var session = 0L

        @Volatile
        private var closed = false

        fun start() {
            session = NativeCore.open { frame ->
                if (!closed) output.offer(CH_CORE to frame)
                true
            }
            connections.add(this)
            Thread(::writeLoop, "ui-bridge-write").apply { isDaemon = true }.start()
            Thread(::readLoop, "ui-bridge-read").apply { isDaemon = true }.start()
        }

        fun send(channel: Int, message: JsonObject) {
            if (!closed) output.offer(channel to gson.toJson(message).toByteArray())
        }

        private fun writeLoop() {
            val out = DataOutputStream(socket.outputStream.buffered())
            try {
                while (!closed) {
                    val (channel, payload) = output.take()
                    if (channel < 0) break
                    out.writeInt(payload.size)
                    out.writeByte(channel)
                    out.write(payload)
                    if (output.isEmpty()) out.flush()
                }
            } catch (_: Exception) {
            } finally {
                close()
            }
        }

        private fun readLoop() {
            val input = DataInputStream(socket.inputStream.buffered())
            try {
                while (!closed) {
                    val length = input.readInt()
                    if (length < 0 || length > MAX_FRAME) break
                    val channel = input.readUnsignedByte()
                    val payload = ByteArray(length)
                    input.readFully(payload)
                    when (channel) {
                        CH_CORE -> if (session == 0L || !NativeCore.send(session, payload)) {
                            GlobalState.log("UI bridge: the Core refused a frame")
                        }
                        CH_SHELL -> handle(payload)
                    }
                }
            } catch (_: Exception) {
            } finally {
                close()
            }
        }

        fun close() {
            if (closed) return
            closed = true
            connections.remove(this)
            output.offer(-1 to ByteArray(0))
            val ended = session
            session = 0L
            if (ended != 0L) NativeCore.close(ended)
            runCatching { socket.close() }
        }

        private fun handle(payload: ByteArray) {
            val request = runCatching { JsonParser.parseString(String(payload)).asJsonObject }.getOrNull() ?: return
            val id = request.get("id")
            GlobalState.launch {
                val answer = runCatching { command(request) }.getOrElse { error ->
                    JsonObject().apply {
                        addProperty("ok", false)
                        addProperty("error", error.message ?: error.toString())
                    }
                }
                if (id != null) answer.add("id", id)
                send(CH_SHELL, answer)
            }
        }
    }

    private fun ok(block: JsonObject.() -> Unit = {}) = JsonObject().apply {
        addProperty("ok", true)
        block()
    }

    private suspend fun command(request: JsonObject): JsonObject = when (request.get("op")?.asString) {
        "hello" -> {
            withContext(Dispatchers.IO) { extractGeoAssets() }
            ok {
                addProperty("home", GlobalState.application.filesDir.path)
                addProperty("sdkInt", Build.VERSION.SDK_INT)
                addProperty("versionName", versionName())
                addProperty("state", ServiceState.runState.value.name)
                addProperty("wifi", onWifi)
                edges?.let { add("insets", it) }
                systemSeeds()?.let { add("seeds", it) }
            }
        }

        "vpn.state" -> ok { addProperty("state", ServiceState.runState.value.name) }

        "vpn.start" -> {
            val options = gson.fromJson(request.get("vpnOptions"), VpnOptions::class.java)
                ?: error("vpnOptions missing")
            if (options.enable && !prepareVpn()) {
                error("需要VPN权限")
            }
            requestNotificationPermission()
            val setup = request.get("setupParams")?.takeIf { it.isJsonObject }?.asJsonObject
            val state = SharedState(
                currentProfileName = request.get("profileName")?.asString ?: "SeasonCat",
                stopText = "暂停",
                startTip = "正在启动VPN…",
                stopTip = "正在停止VPN…",
                vpnOptions = options,
                setupParams = setup,
                onlyStatisticsProxy = request.get("onlyStatisticsProxy")?.asBoolean ?: false,
                showStopAction = request.get("showStopAction")?.asBoolean ?: true,
            )
            ServiceState.syncSharedState(state)
            persistForQuickStart(state)
            if (!ServiceState.requestStart().await()) error("启动服务失败")
            ok { addProperty("state", ServiceState.runState.value.name) }
        }

        "vpn.stop" -> {
            ServiceState.requestStop().await()
            ok { addProperty("state", ServiceState.runState.value.name) }
        }

        "clipboard.set" -> {
            val text = request.get("text")?.asString ?: ""
            withContext(Dispatchers.Main) {
                val clipboard = GlobalState.application
                    .getSystemService(Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager
                clipboard.setPrimaryClip(android.content.ClipData.newPlainText("SeasonCat", text))
            }
            ok()
        }

        "clipboard.get" -> {
            val text = withContext(Dispatchers.Main) {
                val app = GlobalState.application
                val clipboard = app.getSystemService(Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager
                clipboard.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(app)?.toString() ?: ""
            }
            ok { addProperty("text", text) }
        }

        // AppPlugin's getPackages and isInstalledAppsPermissionGranted, for the access control page.
        "packages.list" -> {
            val (packages, granted) = withContext(Dispatchers.IO) {
                packageResolver.installedPackages to packageResolver.hasInstalledAppsPermission()
            }
            ok {
                add("packages", gson.toJsonTree(packages))
                addProperty("granted", granted)
            }
        }

        // getPackageIcon, as PNG files the Rust side can decode: each name's file, or "" when
        // the package has no icon to give.
        "packages.icons" -> {
            val names = request.getAsJsonArray("names")?.map { it.asString } ?: emptyList()
            val paths = withContext(Dispatchers.IO) { names.associateWith { pngIconPath(it) } }
            ok { add("paths", gson.toJsonTree(paths)) }
        }

        // getChinaPackageNames: the installed apps `intelligentSelected` keeps out of the VPN.
        "packages.china" -> {
            val names = withContext(Dispatchers.IO) { packageResolver.getChinaPackageNames() }
            ok { add("packages", gson.toJsonTree(names)) }
        }

        // requestInstalledAppsPermission.
        "packages.request" -> {
            val granted = packageResolver.hasInstalledAppsPermission() || run {
                val host = activity ?: error("界面未就绪")
                withContext(Dispatchers.Main) {
                    suspendCancellableCoroutine { continuation ->
                        host.requestPermission(PackageResolver.GET_INSTALLED_APPS) { granted ->
                            if (continuation.isActive) continuation.resume(granted)
                        }
                    }
                }
            }
            if (granted) packageResolver.invalidate()
            ok { addProperty("granted", granted) }
        }

        // openAppSettings: this app's page in the system settings.
        "app.settings" -> {
            val app = GlobalState.application
            val intent = android.content.Intent(
                android.provider.Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                android.net.Uri.fromParts("package", app.packageName, null),
            ).addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
            withContext(Dispatchers.Main) { app.startActivity(intent) }
            ok()
        }

        // The picked document is copied to `dest` (a path inside the app's files), as
        // FlClash's picker.pickerFile reads its bytes.
        "file.open" -> {
            val dest = File(request.get("dest")?.asString ?: error("dest missing"))
            val intent = android.content.Intent(android.content.Intent.ACTION_OPEN_DOCUMENT)
                .addCategory(android.content.Intent.CATEGORY_OPENABLE)
                .setType("*/*")
            val uri = documentFor(intent)
            if (uri == null) {
                ok { addProperty("cancelled", true) }
            } else {
                val resolver = GlobalState.application.contentResolver
                val name = withContext(Dispatchers.IO) {
                    dest.parentFile?.mkdirs()
                    resolver.openInputStream(uri)?.use { input ->
                        dest.outputStream().use { output -> input.copyTo(output, 1 shl 16) }
                    } ?: error("无法读取文件")
                    resolver.query(uri, arrayOf(android.provider.OpenableColumns.DISPLAY_NAME), null, null, null)
                        ?.use { cursor -> if (cursor.moveToFirst()) cursor.getString(0) else null }
                }
                ok { addProperty("name", name ?: "") }
            }
        }

        // picker.saveFile: the user picks where `src` is written, under `name`.
        "file.save" -> {
            val src = File(request.get("src")?.asString ?: error("src missing"))
            val intent = android.content.Intent(android.content.Intent.ACTION_CREATE_DOCUMENT)
                .addCategory(android.content.Intent.CATEGORY_OPENABLE)
                .setType("application/octet-stream")
                .putExtra(android.content.Intent.EXTRA_TITLE, request.get("name")?.asString ?: src.name)
            val uri = documentFor(intent)
            if (uri == null) {
                ok { addProperty("cancelled", true) }
            } else {
                withContext(Dispatchers.IO) {
                    GlobalState.application.contentResolver.openOutputStream(uri, "wt")?.use { output ->
                        src.inputStream().use { input -> input.copyTo(output, 1 shl 16) }
                    } ?: error("无法写入文件")
                }
                ok()
            }
        }

        "url.open" -> {
            val url = request.get("url")?.asString ?: error("url missing")
            val intent = android.content.Intent(android.content.Intent.ACTION_VIEW, android.net.Uri.parse(url))
                .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
            withContext(Dispatchers.Main) { GlobalState.application.startActivity(intent) }
            ok()
        }

        // `hidden` in the general settings: the task leaves the recent apps.
        "app.hidden" -> {
            val hidden = request.get("value")?.asBoolean ?: false
            withContext(Dispatchers.Main) {
                val taskId = activity?.taskId
                val manager = GlobalState.application
                    .getSystemService(Context.ACTIVITY_SERVICE) as android.app.ActivityManager
                manager.appTasks.firstOrNull { task ->
                    @Suppress("DEPRECATION")
                    val id = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) task.taskInfo?.taskId else task.taskInfo?.id
                    id == taskId
                }?.setExcludeFromRecents(hidden)
            }
            ok()
        }

        // WifiSsidPlugin and BatteryPlugin, for the on-demand page.
        "wifi.ssid" -> {
            val ssid = OnDemandHost.ssid()
            ok { addProperty("ssid", ssid) }
        }

        "wifi.permission" -> {
            val permission = OnDemandHost.checkPermission()
            ok { addProperty("permission", permission) }
        }

        "wifi.request" -> {
            val host = activity ?: error("界面未就绪")
            val permission = OnDemandHost.requestPermission(host)
            ok { addProperty("permission", permission) }
        }

        "battery.ignored" -> ok { addProperty("ignored", OnDemandHost.ignoringOptimizations()) }

        "battery.request" -> {
            val host = activity ?: error("界面未就绪")
            val ignored = OnDemandHost.requestIgnoreOptimizations(host)
            ok { addProperty("ignored", ignored) }
        }

        else -> error("unknown op")
    }

    // DynamicColorPlugin's getCorePalette: the system palette's tones 40 and 80 seed the light
    // and dark schemes of a theme that follows the system.
    private fun systemSeeds(): JsonArray? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return null
        val resources = GlobalState.application.resources
        return JsonArray().apply {
            add(resources.getColor(android.R.color.system_accent1_600, null) and 0xFFFFFF)
            add(resources.getColor(android.R.color.system_accent1_200, null) and 0xFFFFFF)
        }
    }

    private fun pngIconPath(packageName: String): String = runCatching {
        val pm = GlobalState.application.packageManager
        val dir = File(GlobalState.application.cacheDir, "slint_icons").apply { mkdirs() }
        val updated = pm.getPackageInfo(packageName, 0).lastUpdateTime
        val file = File(dir, "$packageName-$updated.png")
        if (!file.exists()) {
            dir.listFiles { f -> f.name.startsWith("$packageName-") }?.forEach { it.delete() }
            val bitmap = pm.getApplicationIcon(packageName).toBitmap(96, 96, Bitmap.Config.ARGB_8888)
            val partial = File.createTempFile(file.name, ".tmp", dir)
            try {
                FileOutputStream(partial).use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
                partial.renameTo(file)
            } finally {
                partial.delete()
                bitmap.recycle()
            }
        }
        if (file.exists()) file.absolutePath else ""
    }.getOrDefault("")

    private fun versionName(): String = runCatching {
        val app = GlobalState.application
        @Suppress("DEPRECATION")
        app.packageManager.getPackageInfo(app.packageName, 0).versionName
    }.getOrNull() ?: ""

    private suspend fun documentFor(intent: android.content.Intent): android.net.Uri? {
        val host = activity ?: error("界面未就绪")
        return withContext(Dispatchers.Main) {
            suspendCancellableCoroutine { continuation ->
                host.startForResult(intent) { uri -> if (continuation.isActive) continuation.resume(uri) }
            }
        }
    }

    private suspend fun prepareVpn(): Boolean {
        val host = activity
        if (android.net.VpnService.prepare(GlobalState.application) == null) return true
        if (host == null) return false
        return withContext(Dispatchers.Main) {
            suspendCancellableCoroutine { continuation ->
                host.prepareVpn { granted -> if (continuation.isActive) continuation.resume(granted) }
            }
        }
    }

    private suspend fun requestNotificationPermission() {
        val host = activity ?: return
        withContext(Dispatchers.Main) { host.requestNotificationPermission() }
    }

    // The tile and quick actions start from these preferences when no UI is attached; the
    // setup params are the full config.apply arguments, profile path included.
    private fun persistForQuickStart(state: SharedState) {
        runCatching {
            GlobalState.application
                .getSharedPreferences("FlutterSharedPreferences", Context.MODE_PRIVATE)
                .edit()
                .putString("flutter.sharedState", gson.toJson(state))
                .apply()
        }
    }

    // CoreController.initGeo: the Core reads these from its home; the APK carries them in
    // Flutter's assets.
    private fun extractGeoAssets() {
        val home = GlobalState.application.filesDir
        GEO_ASSETS.forEach { name ->
            val target = File(home, name)
            if (target.exists()) return@forEach
            runCatching {
                val partial = File(home, "$name.part")
                GlobalState.application.assets.open("flutter_assets/assets/data/$name").use { input ->
                    partial.outputStream().use { output -> input.copyTo(output, 1 shl 16) }
                }
                check(partial.renameTo(target)) { "rename failed" }
            }.onFailure { GlobalState.log("Unable to extract $name: $it") }
        }
    }
}
