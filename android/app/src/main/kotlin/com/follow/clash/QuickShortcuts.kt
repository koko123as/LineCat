package com.follow.clash

import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat
import com.follow.clash.common.GlobalState
import com.follow.clash.common.QuickAction
import com.follow.clash.common.quickIntent
import com.follow.clash.common.shortcutId

private val SHORTCUT_ORDER = listOf(QuickAction.START, QuickAction.STOP, QuickAction.TOGGLE)

fun initQuickShortcuts(labels: Map<QuickAction, String>) {
    val shortcuts = SHORTCUT_ORDER.mapIndexed { rank, action ->
        with(ShortcutInfoCompat.Builder(GlobalState.application, action.shortcutId)) {
            setShortLabel(labels.getValue(action))
            setIcon(IconCompat.createWithResource(GlobalState.application, action.shortcutIcon))
            setIntent(action.quickIntent)
            setRank(rank)
            build()
        }
    }
    ShortcutManagerCompat.setDynamicShortcuts(GlobalState.application, shortcuts)
}

// Plain circles: an adaptive icon takes the launcher's square mask on many devices.
private val QuickAction.shortcutIcon: Int
    get() = when (this) {
        QuickAction.START -> R.drawable.ic_shortcut_start
        QuickAction.STOP -> R.drawable.ic_shortcut_stop
        QuickAction.TOGGLE -> R.drawable.ic_shortcut_toggle
    }
