package com.follow.clash

import android.content.Context
import android.graphics.Paint
import android.graphics.Typeface
import android.graphics.fonts.Font
import android.graphics.text.TextRunShaper
import android.os.Build
import android.system.Os
import android.util.Xml
import androidx.annotation.RequiresApi
import java.io.File
import org.xmlpull.v1.XmlPullParser

/** fontique (Slint's font finder) takes Roboto as Android's sans-serif whatever the device sets; Slint reads these
 *  variables when it builds its font collection on the native thread, so they name the system's own files first. */
object SystemFont {
    private const val LATIN = "Aa"
    private const val HAN = "中文"
    private val WEIGHTS = listOf(400, 500, 600, 700)

    fun export(context: Context) {
        val files = runCatching {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) shaped(context) else declared()
        }.getOrDefault(emptyList()).distinct().filter { it.isFile && it.canRead() }
        if (files.isEmpty()) return
        runCatching {
            Os.setenv("SLINT_DEFAULT_FONT", files.first().path, true)
            if (files.size > 1) {
                Os.setenv("SLINT_FONT_PATH", files.drop(1).joinToString(":") { it.path }, true)
            }
        }
    }

    @RequiresApi(Build.VERSION_CODES.S)
    private fun shaped(context: Context): List<File> {
        val latin = WEIGHTS.mapNotNull { fontOf(LATIN, it) }
        // Another face of a collection would put another region's glyphs first; fontique's fallback picks the face.
        val han = fontOf(HAN, 400)?.takeIf { it.ttcIndex == 0 }
        return (latin + listOfNotNull(han)).mapNotNull { fileOf(context, it) }
    }

    @RequiresApi(Build.VERSION_CODES.S)
    private fun fontOf(text: String, weight: Int): Font? {
        val paint = Paint().apply { typeface = Typeface.create(Typeface.DEFAULT, weight, false) }
        val glyphs = TextRunShaper.shapeTextRun(text, 0, text.length, 0, text.length, 0f, 0f, false, paint)
        return if (glyphs.glyphCount() > 0) glyphs.getFont(0) else null
    }

    @RequiresApi(Build.VERSION_CODES.Q)
    private fun fileOf(context: Context, font: Font): File? {
        font.file?.let { return it }
        val buffer = font.buffer.duplicate().apply { rewind() }
        val out = File(context.noBackupFilesDir, "fonts/system-${font.style.weight}-${buffer.remaining()}.ttf")
        if (out.length() == buffer.remaining().toLong()) return out
        out.parentFile?.mkdirs()
        val bytes = ByteArray(buffer.remaining()).also { buffer.get(it) }
        out.writeBytes(bytes)
        return out
    }

    private fun declared(): List<File> {
        val root = System.getenv("ANDROID_ROOT") ?: "/system"
        val xml = File(root, "etc/fonts.xml").takeIf { it.isFile } ?: return emptyList()
        val files = mutableListOf<Pair<Int, File>>()
        xml.inputStream().use { input ->
            val parser = Xml.newPullParser().apply { setInput(input, null) }
            var inSans = false
            while (parser.next() != XmlPullParser.END_DOCUMENT) {
                if (parser.eventType == XmlPullParser.END_TAG && parser.name == "family" && inSans) break
                if (parser.eventType != XmlPullParser.START_TAG) continue
                when (parser.name) {
                    "family" -> inSans = files.isEmpty() && parser.getAttributeValue(null, "name") == "sans-serif"
                    "font" -> if (inSans && parser.getAttributeValue(null, "style") != "italic") {
                        val weight = parser.getAttributeValue(null, "weight")?.toIntOrNull() ?: 400
                        val name = parser.nextText().trim()
                        if (name.isNotEmpty()) files += weight to File(root, "fonts/$name")
                    }
                }
            }
        }
        return files.sortedBy { kotlin.math.abs(it.first - 400) }.map { it.second }
    }
}
