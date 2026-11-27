package com.follow.clash

import android.app.Application
import android.content.Context.MODE_PRIVATE
import android.os.Handler
import android.os.Looper
import android.widget.Toast
import com.follow.clash.models.SharedState
import com.google.gson.Gson

// The names earlier releases wrote, so an update keeps the tile's quick start.
const val QUICK_START_PREFERENCES = "FlutterSharedPreferences"
const val QUICK_START_STATE_KEY = "flutter.sharedState"

val Application.sharedState: SharedState
    get() = try {
        val preferences = getSharedPreferences(QUICK_START_PREFERENCES, MODE_PRIVATE)
        val json = preferences.getString(QUICK_START_STATE_KEY, null)
        Gson().fromJson(json, SharedState::class.java) ?: SharedState()
    } catch (_: Exception) {
        SharedState()
    }

private var lastToast: Toast? = null

fun Application.showToast(text: String?) {
    if (text.isNullOrEmpty()) return
    Handler(Looper.getMainLooper()).post {
        lastToast?.cancel()
        lastToast = Toast.makeText(this, text, Toast.LENGTH_LONG).apply {
            show()
        }
    }
}
