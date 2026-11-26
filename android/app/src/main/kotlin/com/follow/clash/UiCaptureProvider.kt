package com.follow.clash

import androidx.core.content.FileProvider

/** Hands the camera app the file the QR photo goes to; its own class so no library's provider collides with it. */
class UiCaptureProvider : FileProvider()
