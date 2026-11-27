package com.follow.clash

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import com.follow.clash.common.GlobalState
import java.io.File
import kotlin.math.max
import kotlin.math.roundToInt

/** The picture the Slint UI's QR decoder reads: grayscale, its longest side about [LONGEST] px. */
object QrImage {
    private const val LONGEST = 1600

    /** Writes [uri]'s picture to [dest] as one luma byte per pixel; its width and height, or null. */
    fun writeLuma(uri: Uri, dest: File): Pair<Int, Int>? {
        val resolver = GlobalState.application.contentResolver
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        val stream = resolver.openInputStream(uri) ?: return null
        stream.use { BitmapFactory.decodeStream(it, null, bounds) }
        val longest = max(bounds.outWidth, bounds.outHeight)
        if (longest <= 0) return null
        var sample = 1
        while (longest / (sample * 2) >= LONGEST) sample *= 2
        val options = BitmapFactory.Options().apply {
            inSampleSize = sample
            inPreferredConfig = Bitmap.Config.ARGB_8888
        }
        val decoded = resolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, options) } ?: return null
        val scale = LONGEST.toFloat() / max(decoded.width, decoded.height)
        val bitmap = if (scale < 1f) {
            Bitmap.createScaledBitmap(
                decoded,
                (decoded.width * scale).roundToInt().coerceAtLeast(1),
                (decoded.height * scale).roundToInt().coerceAtLeast(1),
                true,
            ).also { if (it !== decoded) decoded.recycle() }
        } else {
            decoded
        }
        val width = bitmap.width
        val height = bitmap.height
        val pixels = IntArray(width * height)
        bitmap.getPixels(pixels, 0, width, 0, 0, width, height)
        bitmap.recycle()
        val luma = ByteArray(pixels.size)
        for (i in pixels.indices) {
            val p = pixels[i]
            val r = (p shr 16) and 0xff
            val g = (p shr 8) and 0xff
            val b = p and 0xff
            luma[i] = ((r * 299 + g * 587 + b * 114) / 1000).toByte()
        }
        dest.parentFile?.mkdirs()
        dest.writeBytes(luma)
        if (uri.scheme == "file") uri.path?.let { File(it).delete() }
        return width to height
    }
}
