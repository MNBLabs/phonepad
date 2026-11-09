package dev.phonepad.phonepad

import android.content.Context
import android.graphics.Rect
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.Build
import android.os.Bundle
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.view.WindowManager
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.net.Inet4Address
import java.net.NetworkInterface

/**
 * The native half of PhonePad.
 *
 * Deliberately small. The input hot path lives in Dart (see
 * docs/ADR-001-architecture.md) because a platform-channel hop per packet would
 * *add* latency, not remove it. What is here is the set of things Dart genuinely
 * cannot reach: display refresh rate, the Wi-Fi latency lock, system gesture
 * exclusion, crisp haptics, and the local networks discovery has to reach.
 */
class MainActivity : FlutterActivity() {

    private companion object {
        const val CHANNEL = "dev.phonepad/platform"

        /** Interface names that are never a local network a PC could be on. */
        val CARRIER_PREFIXES = listOf("rmnet", "ccmni", "v4-", "clat", "tun", "ppp", "dummy", "ifb")
    }

    private var wifiLock: android.net.wifi.WifiManager.WifiLock? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        requestHighestRefreshRate()
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)

        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, CHANNEL)
            .setMethodCallHandler { call, result ->
                try {
                    when (call.method) {
                        "getDeviceName" -> result.success(deviceName())
                        "getLocalNetworks" -> result.success(localNetworks())
                        "setKeepAwake" -> {
                            setKeepAwake(call.argument<Boolean>("on") ?: false)
                            result.success(null)
                        }
                        "setLowLatencyWifi" -> {
                            setLowLatencyWifi(call.argument<Boolean>("on") ?: false)
                            result.success(null)
                        }
                        "setGestureExclusion" -> {
                            setGestureExclusion(call.argument<Boolean>("on") ?: false)
                            result.success(null)
                        }
                        "getDisplayInfo" -> result.success(displayInfo())
                        "haptic" -> {
                            haptic(
                                call.argument<String>("kind") ?: "tick",
                                call.argument<Int>("amplitude") ?: 128
                            )
                            result.success(null)
                        }
                        "rumble" -> {
                            rumble(
                                call.argument<Int>("large") ?: 0,
                                call.argument<Int>("small") ?: 0
                            )
                            result.success(null)
                        }
                        "openUrl" -> {
                            openUrl(call.argument<String>("url"))
                            result.success(null)
                        }
                        else -> result.notImplemented()
                    }
                } catch (e: Exception) {
                    // Never let a platform failure take the app down mid-game.
                    result.error("PLATFORM_ERROR", e.message, null)
                }
            }
    }

    override fun onDestroy() {
        releaseWifiLock()
        super.onDestroy()
    }

    // --- display --------------------------------------------------------------

    /**
     * Samsung's adaptive refresh rate will happily settle at 60 Hz for an app it
     * thinks is idle. A touch controller is exactly the case where that is
     * wrong, so ask for the fastest mode explicitly.
     */
    private fun requestHighestRefreshRate() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return
        val display = display ?: return
        val current = display.mode ?: return

        // Only consider modes at this resolution; switching resolution to chase
        // Hz would be a worse trade.
        val best = display.supportedModes
            .filter {
                it.physicalWidth == current.physicalWidth &&
                    it.physicalHeight == current.physicalHeight
            }
            .maxByOrNull { it.refreshRate }
            ?: return

        window.attributes = window.attributes.apply {
            preferredDisplayModeId = best.modeId
        }
    }

    private fun displayInfo(): Map<String, Any> {
        val display = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) display else null
        val mode = display?.mode
        return mapOf(
            "refreshRate" to (mode?.refreshRate?.toDouble() ?: 60.0),
            "maxRefreshRate" to (
                display?.supportedModes?.maxOfOrNull { it.refreshRate }?.toDouble() ?: 60.0
                ),
            "model" to "${Build.MANUFACTURER} ${Build.MODEL}",
            "androidSdk" to Build.VERSION.SDK_INT
        )
    }

    private fun setKeepAwake(on: Boolean) {
        runOnUiThread {
            if (on) {
                window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            } else {
                window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            }
        }
    }

    /**
     * Stop the system back/home swipes firing when a thumb strays near a screen
     * edge. Losing a match to an accidental "back" is not an acceptable failure
     * mode for a controller.
     */
    private fun setGestureExclusion(on: Boolean) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return
        runOnUiThread {
            val root = window.decorView
            root.systemGestureExclusionRects = if (on) {
                // The platform caps the excluded height per edge; requesting the
                // whole view is the documented way to ask for that maximum.
                listOf(Rect(0, 0, root.width, root.height))
            } else {
                emptyList()
            }
        }
    }

    // --- Wi-Fi ----------------------------------------------------------------

    /**
     * WIFI_MODE_FULL_LOW_LATENCY disables Wi-Fi power save while held. On
     * Samsung hardware this is a real, measurable win — power save can otherwise
     * add tens of milliseconds of jitter to small, frequent packets.
     */
    @Suppress("DEPRECATION")
    private fun setLowLatencyWifi(on: Boolean) {
        if (!on) {
            releaseWifiLock()
            return
        }
        if (wifiLock?.isHeld == true) return

        val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE)
            as? android.net.wifi.WifiManager ?: return

        val mode = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            android.net.wifi.WifiManager.WIFI_MODE_FULL_LOW_LATENCY
        } else {
            android.net.wifi.WifiManager.WIFI_MODE_FULL_HIGH_PERF
        }

        wifiLock = wifi.createWifiLock(mode, "PhonePad:lowLatency").apply {
            setReferenceCounted(false)
            acquire()
        }
    }

    @Suppress("DEPRECATION")
    private fun releaseWifiLock() {
        wifiLock?.let { if (it.isHeld) it.release() }
        wifiLock = null
    }

    /**
     * Every local IPv4 link a PC could be on, with its subnet broadcast address.
     *
     * Discovery used to ask only about `activeNetwork`. When the phone is the
     * hotspot, or shares its connection over USB, the PC sits on a tethering
     * interface that is never the active network: that is the carrier's. So the
     * probe went out over mobile data and the PC never heard it (issue #2).
     *
     * The kernel keeps a `broadcast <subnet>.255` route for every interface in
     * its local table, so a subnet-directed broadcast always leaves on the link
     * it names, whichever network is the default; and Android routes app traffic
     * to tethered clients through its local-network table, so the PC's unicast
     * replies and the session that follows work too.
     *
     * Cellular and VPN links are left out by transport, not by address: a
     * carrier commonly hands out private 10.x addresses, and announcing the
     * phone's name to the carrier's network helps nobody.
     */
    @Suppress("DEPRECATION")
    private fun localNetworks(): Map<String, Any?> {
        val cm = applicationContext.getSystemService(Context.CONNECTIVITY_SERVICE)
            as? ConnectivityManager

        val excluded = mutableSetOf<String>()
        val wifi = mutableSetOf<String>()
        val ethernet = mutableSetOf<String>()
        var defaultKind = "none"

        if (cm != null) {
            val active = cm.activeNetwork
            for (network in cm.allNetworks) {
                val caps = cm.getNetworkCapabilities(network) ?: continue
                val name = cm.getLinkProperties(network)?.interfaceName ?: continue
                val kind = when {
                    caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN) -> "vpn"
                    caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> "cellular"
                    caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "wifi"
                    caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> "ethernet"
                    else -> "other"
                }
                when (kind) {
                    "vpn", "cellular" -> excluded += name
                    "wifi" -> wifi += name
                    "ethernet" -> ethernet += name
                }
                if (network == active) defaultKind = kind
            }
        }

        val out = mutableListOf<Map<String, Any?>>()
        val interfaces = try {
            NetworkInterface.getNetworkInterfaces()?.toList().orEmpty()
        } catch (e: Exception) {
            emptyList()
        }
        for (ni in interfaces) {
            val name = ni.name ?: continue
            val usable = try {
                ni.isUp && !ni.isLoopback && !ni.isPointToPoint && !ni.isVirtual
            } catch (e: Exception) {
                false
            }
            if (!usable || name in excluded) continue
            // Belt and braces for links that are not exposed as a Network at
            // all: modem data links, the 464xlat CLAT interface, tunnels.
            if (CARRIER_PREFIXES.any { name.startsWith(it) }) continue

            for (ia in ni.interfaceAddresses) {
                val addr = ia.address as? Inet4Address ?: continue
                val broadcast = ia.broadcast as? Inet4Address ?: continue
                if (addr.isLoopbackAddress || addr.isLinkLocalAddress) continue
                if (ia.networkPrefixLength !in 8..30) continue
                out += mapOf(
                    "interface" to name,
                    "address" to addr.hostAddress,
                    "prefixLength" to ia.networkPrefixLength.toInt(),
                    "broadcast" to broadcast.hostAddress,
                    "kind" to kindOf(name, wifi, ethernet)
                )
            }
        }
        return mapOf("networks" to out, "defaultKind" to defaultKind)
    }

    private fun kindOf(name: String, wifi: Set<String>, ethernet: Set<String>): String =
        when {
            name in wifi -> "wifi"
            name in ethernet -> "ethernet"
            // Tethering interfaces are named by the driver, not by any API.
            // These are the names AOSP and the major vendors use.
            name.startsWith("rndis") || name.startsWith("ncm") ||
                name.startsWith("usb") -> "usb"
            name.startsWith("bt-pan") || name.startsWith("bt") -> "bluetooth"
            name.startsWith("swlan") || name.startsWith("ap") ||
                name.startsWith("softap") || name.startsWith("wlan") -> "hotspot"
            else -> "other"
        }

    // --- haptics ---------------------------------------------------------------

    /** Strength currently being played, so an unchanged level is not restarted. */
    private var rumbleLevel = 0

    /**
     * Game rumble, as a held level rather than an event.
     *
     * A phone has one motor where a pad has two, so the strong and weak motors
     * are combined: the strong one dominates and the weak one adds a little on
     * top, which keeps a light effect distinguishable from a heavy one instead
     * of flattening both to "buzzing".
     *
     * The effect is a long one-shot restarted only when the level actually
     * changes. Re-issuing on every feedback packet would retrigger the motor 20
     * times a second and turn a steady rumble into a rattle.
     */
    private fun rumble(large: Int, small: Int) {
        val vibrator = obtainVibrator() ?: return
        if (!vibrator.hasVibrator()) return

        val level = (large.coerceIn(0, 255) + small.coerceIn(0, 255) / 3).coerceIn(0, 255)
        if (level == rumbleLevel) return
        rumbleLevel = level

        if (level == 0) {
            vibrator.cancel()
            return
        }
        // Longer than the interval between feedback packets, so a continuous
        // rumble does not fall into a gap; the watchdog and the level==0 path
        // both cancel it, so it cannot outlive the game asking for it.
        val effect = VibrationEffect.createOneShot(400L, level)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            // Game rumble is not touch feedback. Filed under TOUCH it would be
            // silenced by the system's "vibrate on tap" setting, which the user
            // turned off to stop the keyboard buzzing, not to disable a game.
            vibrator.vibrate(
                effect,
                android.os.VibrationAttributes.createForUsage(
                    android.os.VibrationAttributes.USAGE_MEDIA
                )
            )
        } else {
            vibrator.vibrate(effect)
        }
    }


    /**
     * Short, crisp feedback. Composition primitives feel like a real button on
     * hardware that supports them; everything else falls back to a one-shot.
     */
    private fun haptic(kind: String, amplitude: Int) {
        val vibrator = obtainVibrator() ?: return
        if (!vibrator.hasVibrator()) return

        val clamped = amplitude.coerceIn(1, 255)

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R &&
            vibrator.areAllPrimitivesSupported(VibrationEffect.Composition.PRIMITIVE_CLICK)
        ) {
            val scale = clamped / 255f
            val composition = VibrationEffect.startComposition()
            when (kind) {
                "press" -> composition.addPrimitive(
                    VibrationEffect.Composition.PRIMITIVE_CLICK, scale
                )
                "release" -> composition.addPrimitive(
                    VibrationEffect.Composition.PRIMITIVE_TICK, scale * 0.6f
                )
                "connect" -> composition
                    .addPrimitive(VibrationEffect.Composition.PRIMITIVE_TICK, scale * 0.5f)
                    .addPrimitive(VibrationEffect.Composition.PRIMITIVE_CLICK, scale, 60)
                "disconnect" -> composition
                    .addPrimitive(VibrationEffect.Composition.PRIMITIVE_CLICK, scale, 0)
                    .addPrimitive(VibrationEffect.Composition.PRIMITIVE_TICK, scale * 0.5f, 90)
                else -> composition.addPrimitive(
                    VibrationEffect.Composition.PRIMITIVE_TICK, scale
                )
            }
            vibrator.vibrate(composition.compose())
            return
        }

        val ms = when (kind) {
            "press" -> 12L
            "connect", "disconnect" -> 30L
            else -> 8L
        }
        vibrator.vibrate(VibrationEffect.createOneShot(ms, clamped))
    }

    @Suppress("DEPRECATION")
    private fun obtainVibrator(): Vibrator? =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            (getSystemService(Context.VIBRATOR_MANAGER_SERVICE) as? VibratorManager)?.defaultVibrator
        } else {
            getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator
        }

    /// Hand a URL to the browser. Restricted to http(s) so a malformed or
    /// hostile value cannot be turned into an intent against another app.
    private fun openUrl(url: String?) {
        val uri = android.net.Uri.parse(url ?: return)
        if (uri.scheme != "https" && uri.scheme != "http") return
        startActivity(
            android.content.Intent(android.content.Intent.ACTION_VIEW, uri)
                .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
        )
    }

    private fun deviceName(): String {
        val manufacturer = Build.MANUFACTURER.replaceFirstChar { it.uppercase() }
        val model = Build.MODEL
        return if (model.startsWith(manufacturer, ignoreCase = true)) model
        else "$manufacturer $model"
    }
}
