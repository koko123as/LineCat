package com.follow.clash

import android.graphics.Rect
import android.view.View
import androidx.core.view.WindowInsetsCompat
import com.google.gson.JsonObject

/** The window's own bars, plus the bottom the system hides without a keyboard (Flyme's small window). */
object WindowEdges {
    fun of(view: View): JsonObject? {
        val raw = view.rootWindowInsets ?: return null
        val insets = WindowInsetsCompat.toWindowInsetsCompat(raw, view)
        val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
        var bottom = bars.bottom
        if (!insets.isVisible(WindowInsetsCompat.Type.ime()) && view.height > 0) {
            val visible = Rect()
            view.getWindowVisibleDisplayFrame(visible)
            val origin = IntArray(2)
            view.getLocationOnScreen(origin)
            val hidden = origin[1] + view.height - visible.bottom
            if (hidden in 1 until view.height / 3) bottom = maxOf(bottom, hidden)
        }
        return JsonObject().apply {
            addProperty("ev", "insets")
            addProperty("top", bars.top)
            addProperty("bottom", bottom)
            addProperty("left", bars.left)
            addProperty("right", bars.right)
        }
    }
}
