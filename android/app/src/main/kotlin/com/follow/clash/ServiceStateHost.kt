package com.follow.clash

import android.net.VpnService
import com.follow.clash.common.GlobalState
import com.follow.clash.models.SharedState
import com.follow.clash.service.ServiceConfig
import com.follow.clash.service.models.NotificationParams
import com.follow.clash.service.models.VpnOptions
import kotlinx.coroutines.CoroutineScope

internal data class ServiceRuntime(
    val vpn: Boolean,
    val startedAtMillis: Long,
)

/**
 * The machine owns the arbitration and the transitions; this is only the part that cannot run on a
 * plain JVM, so unit tests can drive the state machine with an in-memory implementation.
 */
internal interface ServiceStateHost {
    val scope: CoroutineScope
    val runtime: ServiceRuntime?
    val homeDirPath: String
    val sdkInt: Int

    fun log(message: String)

    fun showToast(message: String)

    fun updateNotificationParams(params: NotificationParams)

    fun updateMitm(enabled: Boolean)

    fun loadSharedState(): SharedState

    fun isVpnPermissionGranted(): Boolean

    fun tile(): TileGateway?

    fun app(): AppGateway?

    fun notifyStopped(failure: StartFailure? = null)

    suspend fun quickSetup(initParams: String, setupParams: String): Result<String>

    suspend fun startService(options: VpnOptions): Boolean

    suspend fun stopService()
}

internal fun interface TileGateway {
    fun handleStart()
}

internal interface AppGateway {
    fun requestNotificationPermission(callback: (Boolean) -> Unit)

    fun requestLocalNetworkPermission(callback: (Boolean) -> Unit)

    fun prepareVpn(enable: Boolean, callback: (Boolean) -> Unit)

    fun cancelVpnPreparation(callback: (Boolean) -> Unit)
}

/**
 * The Slint UI asks for the VPN and notification permissions itself (UiBridge `vpn.start`) and
 * follows [ServiceState.runState] over the bridge, so no UI gateway is handed to the machine: the
 * tile, shortcuts and quick actions start from the saved shared state.
 */
internal object AndroidServiceStateHost : ServiceStateHost {
    override val scope: CoroutineScope
        get() = GlobalState

    override val runtime: ServiceRuntime?
        get() = ServiceController.runtime

    override val homeDirPath: String
        get() = GlobalState.application.filesDir.path

    override val sdkInt: Int
        get() = android.os.Build.VERSION.SDK_INT

    override fun log(message: String) = GlobalState.log(message)

    override fun showToast(message: String) = GlobalState.application.showToast(message)

    override fun updateNotificationParams(params: NotificationParams) =
        ServiceConfig.updateNotificationParams(params)

    override fun updateMitm(enabled: Boolean) = ServiceConfig.updateMitm(enabled)

    override fun loadSharedState(): SharedState = GlobalState.application.sharedState

    override fun isVpnPermissionGranted(): Boolean =
        VpnService.prepare(GlobalState.application) == null

    override fun tile(): TileGateway? = null

    override fun app(): AppGateway? = null

    override fun notifyStopped(failure: StartFailure?) = Unit

    override suspend fun quickSetup(initParams: String, setupParams: String): Result<String> =
        ServiceController.quickSetup(initParams, setupParams)

    override suspend fun startService(options: VpnOptions): Boolean =
        ServiceController.start(options)

    override suspend fun stopService() = ServiceController.stop()
}
