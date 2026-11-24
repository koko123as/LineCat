package com.follow.clash

import android.app.Activity
import android.graphics.Point
import android.graphics.Rect
import android.os.Build
import android.view.View
import androidx.core.view.WindowInsetsCompat
import com.google.gson.JsonObject

/** The bars this window sits under, counted only as far as it reaches into them: Slint's safe area
 *  (getCurrentWindowMetrics) still carries the whole screen's in Flyme's small window. */
object WindowEdges {
    fun of(activity: Activity, view: View): JsonObject? {
        val raw = view.rootWindowInsets ?: return null
        val bars = WindowInsetsCompat.toWindowInsetsCompat(raw, view).getInsets(
            WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout(),
        )
        val (window, screen) = bounds(activity, view)
        return JsonObject().apply {
            addProperty("ev", "insets")
            addProperty("top", reach(bars.top, window.top - screen.top))
            addProperty("bottom", reach(bars.bottom, screen.bottom - window.bottom))
            addProperty("left", reach(bars.left, window.left - screen.left))
            addProperty("right", reach(bars.right, screen.right - window.right))
            addProperty("width", view.width)
            addProperty("height", view.height)
        }
    }

    private fun reach(inset: Int, gap: Int): Int = (inset - gap.coerceAtLeast(0)).coerceIn(0, inset.coerceAtLeast(0))

    private fun bounds(activity: Activity, view: View): Pair<Rect, Rect> {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val manager = activity.windowManager
            return manager.currentWindowMetrics.bounds to manager.maximumWindowMetrics.bounds
        }
        val at = IntArray(2)
        view.getLocationOnScreen(at)
        val size = Point()
        @Suppress("DEPRECATION")
        activity.windowManager.defaultDisplay.getRealSize(size)
        return Rect(at[0], at[1], at[0] + view.width, at[1] + view.height) to Rect(0, 0, size.x, size.y)
    }
}
