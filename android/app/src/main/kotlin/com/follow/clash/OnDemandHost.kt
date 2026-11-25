package com.follow.clash

import android.Manifest
import android.annotation.SuppressLint
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.wifi.WifiInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.core.content.edit
import androidx.core.net.toUri
import com.follow.clash.common.GlobalState
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import java.util.concurrent.ConcurrentHashMap
import kotlin.coroutines.resume

/** WifiSsidPlugin's SSID and location permission and BatteryPlugin's exemption, for the Slint UI. */
object OnDemandHost {
    private const val PERMISSION_GRANTED = 0
    private const val PERMISSION_DENIED = 1
    private const val PERMISSION_PERMANENTLY_DENIED = 2
    private const val DENIED_PERMISSIONS_PREFERENCES = "wifi_ssid_denied_permissions"
    private const val SSID_TIMEOUT_MILLIS = 3_000L
    private const val MILLET_NO_RESTRICT_APP = "MILLET_NO_RESTRICT_APP"
    private const val POWERKEEPER_PACKAGE = "com.miui.powerkeeper"
    private const val POWERKEEPER_POLICY_ACTIVITY = "com.miui.powerkeeper.ui.HiddenAppsConfigActivity"
    private val isXiaomi = Build.MANUFACTURER.equals("xiaomi", ignoreCase = true)

    private val context: Context get() = GlobalState.application
    private val connectivity by lazy { context.getSystemService(ConnectivityManager::class.java) }
    private val wifiInfoByNetwork = ConcurrentHashMap<Network, WifiInfo>()
    private var wifiCallback: ConnectivityManager.NetworkCallback? = null

    suspend fun ssid(): String? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) {
            val manager = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
            @Suppress("DEPRECATION")
            return normalize(manager?.connectionInfo?.ssid)
        }
        if (!hasForegroundLocation()) return null
        currentSsid()?.let { return it }
        if (!withContext(Dispatchers.Main) { registerWifiCallback() }) return null
        return withTimeoutOrNull(SSID_TIMEOUT_MILLIS) {
            var ssid = currentSsid()
            while (ssid == null) {
                delay(100)
                ssid = currentSsid()
            }
            ssid
        } ?: currentSsid()
    }

    private fun currentSsid(): String? {
        val info = connectivity?.activeNetwork?.let(wifiInfoByNetwork::get)
            ?: wifiInfoByNetwork.values.firstOrNull()
        return normalize(info?.ssid)
    }

    private fun normalize(ssid: String?): String? =
        ssid?.removeSurrounding("\"")?.takeIf {
            it.isNotEmpty() && it != WifiManager.UNKNOWN_SSID && it != "0x"
        }

    private fun registerWifiCallback(): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return false
        if (!hasForegroundLocation()) {
            unregisterWifiCallback()
            return false
        }
        if (wifiCallback != null) return true
        val manager = connectivity ?: return false
        val callback = object : ConnectivityManager.NetworkCallback(FLAG_INCLUDE_LOCATION_INFO) {
            override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
                val info = capabilities.transportInfo as? WifiInfo
                if (info == null) wifiInfoByNetwork.remove(network) else wifiInfoByNetwork[network] = info
            }

            override fun onLost(network: Network) {
                wifiInfoByNetwork.remove(network)
            }
        }
        val request = NetworkRequest.Builder().addTransportType(NetworkCapabilities.TRANSPORT_WIFI).build()
        return runCatching { manager.registerNetworkCallback(request, callback) }
            .onSuccess { wifiCallback = callback }
            .isSuccess
    }

    private fun unregisterWifiCallback() {
        val callback = wifiCallback ?: return
        wifiCallback = null
        runCatching { connectivity?.unregisterNetworkCallback(callback) }
        wifiInfoByNetwork.clear()
    }

    private fun refreshWifiCallback() {
        unregisterWifiCallback()
        registerWifiCallback()
    }

    suspend fun checkPermission(): Int = withContext(Dispatchers.Main) {
        refreshWifiCallback()
        forgetGrantedDenials()
        if (missingPermission() == null) PERMISSION_GRANTED else PERMISSION_DENIED
    }

    suspend fun requestPermission(host: SlintActivity): Int = withContext(Dispatchers.Main) {
        if (missingPermission() == null) {
            refreshWifiCallback()
            forgetGrantedDenials()
            return@withContext PERMISSION_GRANTED
        }
        val foreground = !hasForegroundLocation()
        var rationale = if (foreground) {
            ask(host, foregroundPermissions())
        } else {
            ask(host, arrayOf(Manifest.permission.ACCESS_BACKGROUND_LOCATION))
        }
        // R+ dialogs never offer "Allow all the time"; asking for it alone opens Settings.
        val needsBackground = hasForegroundLocation() && !hasBackgroundLocation()
        if (foreground && Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && needsBackground) {
            rationale = ask(host, arrayOf(Manifest.permission.ACCESS_BACKGROUND_LOCATION))
        }
        refreshWifiCallback()
        forgetGrantedDenials()
        val missing = missingPermission() ?: return@withContext PERMISSION_GRANTED
        resolveDenial(host, missing, rationale[missing] == true)
    }

    /** Asks for [permissions] and answers with each one's rationale from before the prompt. */
    private suspend fun ask(host: SlintActivity, permissions: Array<String>): Map<String, Boolean> {
        val before = permissions.associateWith { ActivityCompat.shouldShowRequestPermissionRationale(host, it) }
        suspendCancellableCoroutine<Unit> { continuation ->
            host.askPermissions(permissions) { if (continuation.isActive) continuation.resume(Unit) }
        }
        return before
    }

    private fun foregroundPermissions(): Array<String> =
        // Q accepts the background permission in the same dialog; R+ rejects the combined request.
        if (Build.VERSION.SDK_INT == Build.VERSION_CODES.Q) {
            arrayOf(
                Manifest.permission.ACCESS_FINE_LOCATION,
                Manifest.permission.ACCESS_COARSE_LOCATION,
                Manifest.permission.ACCESS_BACKGROUND_LOCATION,
            )
        } else {
            arrayOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION)
        }

    // Rationale is false both before the first prompt and after a permanent denial;
    // how it moved across the request and a persisted earlier denial tell them apart.
    private fun resolveDenial(host: SlintActivity, permission: String, rationaleBefore: Boolean): Int {
        val preferences = deniedPermissions()
        val deniedBefore = preferences.getBoolean(permission, false)
        if (ActivityCompat.shouldShowRequestPermissionRationale(host, permission)) {
            preferences.edit { putBoolean(permission, true) }
            return PERMISSION_DENIED
        }
        if (rationaleBefore) {
            preferences.edit { putBoolean(permission, true) }
            return PERMISSION_PERMANENTLY_DENIED
        }
        return if (deniedBefore) PERMISSION_PERMANENTLY_DENIED else PERMISSION_DENIED
    }

    private fun forgetGrantedDenials() {
        val preferences = deniedPermissions()
        val granted = preferences.all.keys.filter { isGranted(it) }
        if (granted.isNotEmpty()) preferences.edit { granted.forEach { remove(it) } }
    }

    private fun deniedPermissions() =
        context.getSharedPreferences(DENIED_PERMISSIONS_PREFERENCES, Context.MODE_PRIVATE)

    private fun isGranted(permission: String): Boolean =
        ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED

    private fun hasForegroundLocation(): Boolean = isGranted(Manifest.permission.ACCESS_FINE_LOCATION)

    private fun hasBackgroundLocation(): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.Q || isGranted(Manifest.permission.ACCESS_BACKGROUND_LOCATION)

    private fun missingPermission(): String? = when {
        !hasForegroundLocation() -> Manifest.permission.ACCESS_FINE_LOCATION
        !hasBackgroundLocation() -> Manifest.permission.ACCESS_BACKGROUND_LOCATION
        else -> null
    }

    // On Xiaomi the battery saver's "No restrictions" is what counts, as BatteryPlugin reads it.
    fun ignoringOptimizations(): Boolean {
        if (isXiaomi) {
            val packages = runCatching {
                Settings.System.getString(context.contentResolver, MILLET_NO_RESTRICT_APP)
            }.getOrNull()
            return context.packageName in packages?.split(',')?.map { it.trim() }.orEmpty()
        }
        return context.getSystemService(PowerManager::class.java)
            ?.isIgnoringBatteryOptimizations(context.packageName) ?: false
    }

    /** BatteryPlugin's requestIgnoreOptimizations: the first page that opens, then the answer. */
    suspend fun requestIgnoreOptimizations(host: SlintActivity): Boolean {
        if (ignoringOptimizations()) return true
        val pages = if (isXiaomi) saverPages() else optimizationPages()
        withContext(Dispatchers.Main) {
            for (page in pages) {
                val opened = suspendCancellableCoroutine<Boolean> { continuation ->
                    host.launchForResult(page) { opened -> if (continuation.isActive) continuation.resume(opened) }
                }
                if (opened) break
            }
        }
        return ignoringOptimizations()
    }

    @SuppressLint("BatteryLife")
    private fun optimizationPages(): List<Intent> = listOf(
        // VPN continuity is the user-requested core function, so the direct exemption is intentional.
        Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, "package:${context.packageName}".toUri()),
        Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS),
    )

    private fun saverPages(): List<Intent> = listOf(
        Intent()
            .setComponent(ComponentName(POWERKEEPER_PACKAGE, POWERKEEPER_POLICY_ACTIVITY))
            .putExtra("package_name", context.packageName)
            .putExtra("package_label", context.applicationInfo.loadLabel(context.packageManager).toString()),
        Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, "package:${context.packageName}".toUri()),
    )
}
