package com.follow.clash

import android.app.Activity
import android.graphics.Point
import android.graphics.Rect
import android.os.Build
import android.view.View
import android.view.WindowManager
import androidx.core.view.WindowInsetsCompat
import com.google.gson.JsonObject

/** The bars this window sits under, counted only as far as it reaches into them: Slint's safe area
 *  (getCurrentWindowMetrics) still carries the whole screen's in Flyme's small window. The window's
 *  size and its surface insets go with them: a floating window's surface is larger than the window,
 *  and Slint, drawing the whole surface from its top left, would put the bottom bar outside it. */
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
            addProperty("height", shown(view, window))
            surfaceInsets(activity)?.let { add("surface", it) }
        }
    }

    // Bounds are in the view's pixels only when their width matches the decor's; Flyme's scaled
    // small window reports them on the screen's scale.
    private fun shown(view: View, window: Rect): Int =
        if (window.width() == view.width && window.height() in 1 until view.height) window.height() else view.height

    private fun surfaceInsets(activity: Activity): JsonObject? = runCatching {
        val field = WindowManager.LayoutParams::class.java.getField("surfaceInsets")
        val insets = field.get(activity.window.attributes) as? Rect ?: return null
        if (insets.left == 0 && insets.top == 0 && insets.right == 0 && insets.bottom == 0) return null
        JsonObject().apply {
            addProperty("left", insets.left)
            addProperty("top", insets.top)
            addProperty("right", insets.right)
            addProperty("bottom", insets.bottom)
        }
    }.getOrNull()

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
