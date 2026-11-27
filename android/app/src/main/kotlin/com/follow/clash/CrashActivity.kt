package com.follow.clash

import android.app.Activity
import android.os.Bundle
import android.widget.ScrollView
import android.widget.TextView

/** Shows the reason the Slint UI failed last time; runs in its own process. */
class CrashActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val text = intent.getStringExtra("text").orEmpty()
        val view = TextView(this).apply {
            setTextIsSelectable(true)
            setPadding(40, 120, 40, 40)
            textSize = 12f
            setText("SeasonCat 上次启动失败（长按可复制）：\n\n$text")
        }
        setContentView(ScrollView(this).apply { addView(view) })
    }
}
