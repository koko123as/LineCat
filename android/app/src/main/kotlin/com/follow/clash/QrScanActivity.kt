package com.follow.clash

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.util.Size
import android.util.TypedValue
import android.view.Gravity
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.widget.Button
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast
import androidx.camera.core.Camera
import androidx.camera.core.CameraSelector
import androidx.camera.core.ExperimentalGetImage
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.core.SurfaceRequest
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import com.google.mlkit.vision.barcode.BarcodeScanner
import com.google.mlkit.vision.barcode.BarcodeScannerOptions
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.common.InputImage
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import kotlin.math.max
import kotlin.math.min

/**
 * The bridge's side of [QrScanActivity]: `qr.capture` waits here for the codes the page reads,
 * the Rust side judges them (`profileUrlFromQrCodes`), and either closes the page with
 * `qr.close` or asks again with the invalid hint, as FlClash's ScanPage keeps scanning.
 */
object QrScanner {
    sealed interface Outcome {
        data class Codes(val values: List<String>, val album: Boolean) : Outcome

        data object Closed : Outcome
    }

    /** The dark scheme's colours the page draws with, as `watchAppTheme(Brightness.dark)`. */
    data class Colors(val primary: Int, val onPrimary: Int, val surface: Int, val onSurface: Int, val muted: Int, val error: Int)

    var colors = Colors(0xFF8AB4F8.toInt(), Color.BLACK, Color.BLACK, Color.WHITE, 0xFFC4C6CF.toInt(), 0xFFFFB4AB.toInt())
        private set

    private var pending: ((Outcome) -> Unit)? = null
    private var screen: QrScanActivity? = null

    /** Main thread: the next codes, opening the page or resuming the open one. */
    fun request(host: Activity, invalid: String?, colors: Colors?, callback: (Outcome) -> Unit) {
        colors?.let { this.colors = it }
        pending?.invoke(Outcome.Closed)
        pending = callback
        val open = screen
        if (open != null) {
            open.scanAgain(invalid)
        } else {
            host.startActivity(Intent(host, QrScanActivity::class.java))
        }
    }

    fun close() {
        screen?.finish()
    }

    internal fun attach(page: QrScanActivity) {
        screen = page
    }

    internal fun detach(page: QrScanActivity) {
        if (screen === page) {
            screen = null
            deliver(Outcome.Closed)
        }
    }

    internal fun deliver(outcome: Outcome) {
        val callback = pending
        pending = null
        callback?.invoke(outcome)
    }
}

/**
 * FlClash's ScanPage (`mobile_scanner`: CameraX with ML Kit's barcode scanning, QR codes only):
 * the preview behind a scrim with the scan window cut out, the hint under it, 关闭 and 从相册选择
 * in the bar, the torch under the hint, and the permission and unavailable states.
 */
class QrScanActivity : Activity(), LifecycleOwner {
    private val registry = LifecycleRegistry(this)
    override val lifecycle: Lifecycle
        get() = registry

    private lateinit var analysisExecutor: ExecutorService
    private lateinit var scanner: BarcodeScanner
    private lateinit var preview: SurfaceView
    private lateinit var overlay: ScanOverlay
    private lateinit var hint: TextView
    private lateinit var torch: GlyphButton
    private lateinit var closeButton: GlyphButton
    private lateinit var albumButton: GlyphButton
    private lateinit var status: LinearLayout
    private var provider: ProcessCameraProvider? = null
    private var camera: Camera? = null
    private var surfaceRequest: SurfaceRequest? = null
    private var torchOn = false

    @Volatile
    private var paused = false
    private var starting = false
    private var picking = false
    private var deniedToSettings = false
    private var insetTop = 0
    private var insetBottom = 0
    private val handler = android.os.Handler(android.os.Looper.getMainLooper())
    private val clearHint = Runnable { showHint(false) }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        QrScanner.attach(this)
        registry.handleLifecycleEvent(Lifecycle.Event.ON_CREATE)
        analysisExecutor = Executors.newSingleThreadExecutor()
        scanner = BarcodeScanning.getClient(
            BarcodeScannerOptions.Builder().setBarcodeFormats(Barcode.FORMAT_QR_CODE).build(),
        )
        window.statusBarColor = Color.TRANSPARENT
        window.navigationBarColor = Color.TRANSPARENT
        @Suppress("DEPRECATION")
        window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_LAYOUT_STABLE or
            View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
        setContentView(buildContent())
    }

    private fun dp(v: Float) = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v, resources.displayMetrics)

    private fun buildContent(): View {
        val colors = QrScanner.colors
        val root = object : FrameLayout(this) {
            override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
                super.onLayout(changed, left, top, right, bottom)
                placeControls(right - left, bottom - top)
            }
        }
        root.setBackgroundColor(Color.BLACK)
        root.clipChildren = true
        preview = SurfaceView(this)
        root.addView(preview, FrameLayout.LayoutParams(0, 0))
        overlay = ScanOverlay(this, colors)
        root.addView(overlay, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        hint = TextView(this).apply {
            gravity = Gravity.CENTER
            maxLines = HINT_LINES
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
        }
        root.addView(hint, FrameLayout.LayoutParams(0, 0))
        showHint(false)
        status = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER
            setBackgroundColor(colors.surface)
            visibility = View.GONE
            val pad = dp(32f).toInt()
            setPadding(pad, 0, pad, 0)
        }
        root.addView(status, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        closeButton = GlyphButton(this, GLYPH_CLOSE, colors, filled = false).apply {
            contentDescription = "关闭"
            setOnClickListener { finish() }
        }
        root.addView(closeButton, FrameLayout.LayoutParams(dp(48f).toInt(), dp(48f).toInt()))
        albumButton = GlyphButton(this, GLYPH_PHOTOS, colors, filled = false).apply {
            contentDescription = "从相册选择"
            setOnClickListener { pickFromAlbum() }
        }
        root.addView(albumButton, FrameLayout.LayoutParams(dp(48f).toInt(), dp(48f).toInt()))
        torch = GlyphButton(this, GLYPH_TORCH, colors, filled = true).apply {
            contentDescription = "手电筒"
            visibility = View.INVISIBLE
            setOnClickListener { toggleTorch() }
        }
        root.addView(torch, FrameLayout.LayoutParams(dp(TORCH_SIZE).toInt(), dp(TORCH_SIZE).toInt()))
        root.setOnApplyWindowInsetsListener { view, insets: WindowInsets ->
            @Suppress("DEPRECATION")
            insetTop = insets.systemWindowInsetTop
            @Suppress("DEPRECATION")
            insetBottom = insets.systemWindowInsetBottom
            view.requestLayout()
            insets
        }
        preview.holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {}

            override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
                offerSurface(width, height)
            }

            override fun surfaceDestroyed(holder: SurfaceHolder) {}
        })
        return root
    }

    // `_ScanLayout._torchBelow`: the window as large as the hint and the torch below it allow.
    private fun placeControls(width: Int, height: Int) {
        val toolbar = dp(TOOLBAR).toInt()
        val button = dp(48f).toInt()
        val barTop = insetTop + (toolbar - button) / 2
        closeButton.layout(dp(12f).toInt(), barTop, dp(12f).toInt() + button, barTop + button)
        albumButton.layout(width - dp(12f).toInt() - button, barTop, width - dp(12f).toInt(), barTop + button)
        val torchSize = dp(TORCH_SIZE).toInt()
        val torchTop = height - max(insetBottom, dp(48f).toInt()) - torchSize
        torch.layout((width - torchSize) / 2, torchTop, (width + torchSize) / 2, torchTop + torchSize)
        val hintHeight = (hint.lineHeight * HINT_LINES)
        val top = insetTop + toolbar
        val bottom = torchTop - dp(16f).toInt() - dp(24f).toInt() - hintHeight
        val side = max(0f, minOf(dp(400f), min(width, height) * 0.67f, (bottom - top).toFloat(), width.toFloat()))
        val centerY = max(top + side / 2, min(height / 2f, bottom - side / 2))
        val window = RectF((width - side) / 2, centerY - side / 2, (width + side) / 2, centerY + side / 2)
        overlay.window = window
        val hintInset = dp(32f).toInt()
        val hintTop = (window.bottom + dp(24f)).toInt()
        hint.measure(
            View.MeasureSpec.makeMeasureSpec(max(0, width - 2 * hintInset), View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(hintHeight, View.MeasureSpec.EXACTLY),
        )
        hint.layout(hintInset, hintTop, width - hintInset, hintTop + hintHeight)
        placePreview(width, height)
    }

    private fun root(): ViewGroup = preview.parent as ViewGroup

    // The preview fills the page as `BoxFit.cover` does, the part outside clipped.
    private fun placePreview(width: Int, height: Int) {
        val request = surfaceRequest
        val size = request?.resolution ?: Size(width, height)
        val turned = (camera?.cameraInfo?.sensorRotationDegrees ?: 90) % 180 != 0
        val w = if (turned) size.height else size.width
        val h = if (turned) size.width else size.height
        val scale = max(width.toFloat() / w, height.toFloat() / h)
        val pw = (w * scale).toInt()
        val ph = (h * scale).toInt()
        val left = (width - pw) / 2
        val top = (height - ph) / 2
        preview.measure(
            View.MeasureSpec.makeMeasureSpec(pw, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(ph, View.MeasureSpec.EXACTLY),
        )
        preview.layout(left, top, left + pw, top + ph)
    }

    override fun onStart() {
        super.onStart()
        registry.handleLifecycleEvent(Lifecycle.Event.ON_START)
    }

    override fun onResume() {
        super.onResume()
        registry.handleLifecycleEvent(Lifecycle.Event.ON_RESUME)
        if (picking) return
        val granted = checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED
        when {
            granted -> start()
            deniedToSettings -> showDenied()
            status.visibility != View.VISIBLE -> requestPermissions(arrayOf(Manifest.permission.CAMERA), PERMISSION_REQUEST)
        }
    }

    override fun onPause() {
        registry.handleLifecycleEvent(Lifecycle.Event.ON_PAUSE)
        super.onPause()
    }

    override fun onStop() {
        registry.handleLifecycleEvent(Lifecycle.Event.ON_STOP)
        torchOn = false
        super.onStop()
    }

    override fun onDestroy() {
        handler.removeCallbacks(clearHint)
        registry.handleLifecycleEvent(Lifecycle.Event.ON_DESTROY)
        provider?.unbindAll()
        scanner.close()
        analysisExecutor.shutdown()
        QrScanner.detach(this)
        super.onDestroy()
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode != PERMISSION_REQUEST) return
        if (grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED) {
            start()
        } else {
            showDenied()
        }
    }

    private fun start() {
        status.visibility = View.GONE
        deniedToSettings = false
        val bound = provider
        if (bound != null) {
            if (camera == null) bind(bound)
            return
        }
        if (starting) return
        starting = true
        val future = ProcessCameraProvider.getInstance(this)
        future.addListener({
            starting = false
            if (isDestroyed) return@addListener
            val ready = runCatching { future.get() }.getOrNull()
            if (ready == null) {
                showUnavailable(retry = true)
                return@addListener
            }
            provider = ready
            bind(ready)
        }, ContextCompat.getMainExecutor(this))
    }

    private fun bind(cameras: ProcessCameraProvider) {
        val selector = CameraSelector.DEFAULT_BACK_CAMERA
        if (!runCatching { cameras.hasCamera(selector) }.getOrDefault(false)) {
            showUnavailable(retry = false)
            return
        }
        val shown = Preview.Builder().build()
        shown.setSurfaceProvider(ContextCompat.getMainExecutor(this)) { request ->
            surfaceRequest?.willNotProvideSurface()
            surfaceRequest = request
            val size = request.resolution
            preview.holder.setFixedSize(size.width, size.height)
            preview.requestLayout()
            offerSurface(preview.holder.surfaceFrame.width(), preview.holder.surfaceFrame.height())
        }
        val analysis = ImageAnalysis.Builder()
            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
            .build()
        analysis.setAnalyzer(analysisExecutor) { image -> analyze(image) }
        val bound = runCatching {
            cameras.unbindAll()
            cameras.bindToLifecycle(this, selector, shown, analysis)
        }.getOrNull()
        if (bound == null) {
            showUnavailable(retry = true)
            return
        }
        camera = bound
        torch.visibility = if (bound.cameraInfo.hasFlashUnit()) View.VISIBLE else View.INVISIBLE
        root().requestLayout()
    }

    private fun offerSurface(width: Int, height: Int) {
        val request = surfaceRequest ?: return
        val size = request.resolution
        if (width != size.width || height != size.height) return
        val surface = preview.holder.surface
        if (surface == null || !surface.isValid) return
        surfaceRequest = null
        request.provideSurface(surface, ContextCompat.getMainExecutor(this)) { }
    }

    @androidx.annotation.OptIn(markerClass = [ExperimentalGetImage::class])
    private fun analyze(image: ImageProxy) {
        val media = image.image
        if (media == null || paused) {
            image.close()
            return
        }
        val input = InputImage.fromMediaImage(media, image.imageInfo.rotationDegrees)
        scanner.process(input)
            .addOnSuccessListener { codes -> found(codes.mapNotNull { it.rawValue }, album = false) }
            .addOnCompleteListener { image.close() }
    }

    // `DetectionSpeed.noDuplicates` stops at the first read; the Rust side answers whether
    // it holds a profile, and an invalid one brings [scanAgain].
    private fun found(values: List<String>, album: Boolean) {
        runOnUiThread {
            if (paused || isFinishing || values.isEmpty() && !album) return@runOnUiThread
            paused = true
            QrScanner.deliver(QrScanner.Outcome.Codes(values, album))
        }
    }

    /** The codes were no profile: the hint turns red for two seconds, or an album pick toasts. */
    fun scanAgain(invalid: String?) {
        when (invalid) {
            "album" -> Toast.makeText(this, "请上传有效的二维码", Toast.LENGTH_SHORT).show()
            null, "" -> {}
            else -> {
                showHint(true)
                handler.removeCallbacks(clearHint)
                handler.postDelayed(clearHint, INVALID_HINT_MS)
            }
        }
        paused = false
    }

    private fun showHint(invalid: Boolean) {
        val colors = QrScanner.colors
        hint.text = if (invalid) "该二维码不包含配置文件链接" else "扫描二维码获取配置文件"
        hint.setTextColor(if (invalid) colors.error else colors.muted)
    }

    private fun toggleTorch() {
        val bound = camera ?: return
        torchOn = !torchOn
        bound.cameraControl.enableTorch(torchOn)
        torch.lit = torchOn
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != ALBUM_REQUEST) return
        picking = false
        val uri = data?.data
        if (resultCode != RESULT_OK || uri == null) return
        readAlbum(uri)
    }

    // `pickerConfigQRCode`: the picked image read by the same scanner.
    private fun pickFromAlbum() {
        if (picking) return
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT)
            .addCategory(Intent.CATEGORY_OPENABLE)
            .setType("image/*")
        picking = runCatching { startActivityForResult(intent, ALBUM_REQUEST) }.isSuccess
    }

    private fun readAlbum(uri: Uri) {
        val input = runCatching { InputImage.fromFilePath(this, uri) }.getOrNull()
        if (input == null) {
            Toast.makeText(this, "请上传有效的二维码", Toast.LENGTH_SHORT).show()
            return
        }
        paused = true
        scanner.process(input)
            .addOnSuccessListener { codes ->
                paused = false
                found(codes.mapNotNull { it.rawValue }, album = true)
            }
            .addOnFailureListener {
                paused = false
                Toast.makeText(this, "请上传有效的二维码", Toast.LENGTH_SHORT).show()
            }
    }

    private fun showDenied() {
        showStatus("需要相机权限", "请在系统设置中允许访问相机以扫描二维码，或从相册选择二维码图片。", "设置") {
            deniedToSettings = true
            startActivity(
                Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", packageName, null)),
            )
        }
    }

    private fun showUnavailable(retry: Boolean) {
        showStatus("相机不可用", null, if (retry) "重试" else null) { start() }
    }

    private fun showStatus(title: String, description: String?, action: String?, onAction: () -> Unit) {
        val colors = QrScanner.colors
        torch.visibility = View.INVISIBLE
        status.removeAllViews()
        status.addView(
            TextView(this).apply {
                text = title
                gravity = Gravity.CENTER
                setTextColor(colors.onSurface)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 20f)
            },
        )
        if (description != null) {
            status.addView(
                TextView(this).apply {
                    text = description
                    gravity = Gravity.CENTER
                    setTextColor(colors.muted)
                    setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
                    setPadding(0, dp(8f).toInt(), 0, 0)
                },
            )
        }
        if (action != null) {
            status.addView(
                Button(this).apply {
                    text = action
                    isAllCaps = false
                    setTextColor(colors.onPrimary)
                    background = android.graphics.drawable.GradientDrawable().apply {
                        setColor(colors.primary)
                        cornerRadius = dp(24f)
                    }
                    setOnClickListener { onAction() }
                },
                LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, dp(48f).toInt()).apply {
                    topMargin = dp(24f).toInt()
                },
            )
        }
        status.visibility = View.VISIBLE
    }

    private companion object {
        const val PERMISSION_REQUEST = 0x5143
        const val ALBUM_REQUEST = 0x5144
        const val INVALID_HINT_MS = 2000L
        const val HINT_LINES = 2
        const val TORCH_SIZE = 64f
        const val TOOLBAR = 72f
        const val GLYPH_CLOSE = "M6 6L18 18M18 6L6 18"
        const val GLYPH_PHOTOS = "M4 5h16v14H4z M4 16l5-5 4 4 2-2 5 5 M15 9.5a1.5 1.5 0 1 0 0.01 0"
        const val GLYPH_TORCH = "M8 3h8v4l-2 3v11h-4V10L8 7z M12 13v2"
    }
}

/** `ScannerOverlay` and the window's border: the scrim with the scan window cut out. */
private class ScanOverlay(context: Context, private val colors: QrScanner.Colors) : View(context) {
    var window: RectF? = null
        set(value) {
            field = value
            invalidate()
        }
    private val scrim = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.argb(153, 0, 0, 0) }
    private val border = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        color = colors.primary
    }
    private val path = Path()

    override fun onDraw(canvas: Canvas) {
        val w = window ?: return
        val radius = w.width() * 0.08f
        border.strokeWidth = 2.5f * resources.displayMetrics.density
        path.reset()
        path.fillType = Path.FillType.EVEN_ODD
        path.addRect(0f, 0f, width.toFloat(), height.toFloat(), Path.Direction.CW)
        path.addRoundRect(w, radius, radius, Path.Direction.CW)
        canvas.drawPath(path, scrim)
        canvas.drawRoundRect(w, radius, radius, border)
    }
}

/** A round button with a stroked glyph on a 24-unit grid; [filled] marks the torch's tonal plate. */
private class GlyphButton(
    context: Context,
    glyph: String,
    private val colors: QrScanner.Colors,
    private val filled: Boolean,
) : View(context) {
    var lit = false
        set(value) {
            field = value
            invalidate()
        }
    private val path = parseGlyph(glyph)
    private val plate = Paint(Paint.ANTI_ALIAS_FLAG)
    private val stroke = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeCap = Paint.Cap.ROUND
        strokeJoin = Paint.Join.ROUND
    }

    init {
        isClickable = true
        isFocusable = true
    }

    override fun onDraw(canvas: Canvas) {
        val size = min(width, height).toFloat()
        plate.color = when {
            lit -> colors.primary
            filled -> Color.argb(72, 255, 255, 255)
            else -> Color.argb(56, 0, 0, 0)
        }
        canvas.drawCircle(width / 2f, height / 2f, size / 2, plate)
        stroke.color = if (lit) colors.onPrimary else Color.WHITE
        val icon = size * 0.5f
        val scale = icon / 24f
        canvas.save()
        canvas.translate((width - icon) / 2, (height - icon) / 2)
        canvas.scale(scale, scale)
        stroke.strokeWidth = 2f
        canvas.drawPath(path, stroke)
        canvas.restore()
    }

    private companion object {
        // M / L / H / V / h / v / l / z and the circle's `a` drawn as a dot, enough for the glyphs above.
        fun parseGlyph(d: String): Path {
            val path = Path()
            val tokens = Regex("[A-Za-z]|-?[0-9]*\\.?[0-9]+").findAll(d).map { it.value }.toList()
            var i = 0
            var cmd = 'M'
            var x = 0f
            var y = 0f
            fun num() = tokens[i++].toFloat()
            while (i < tokens.size) {
                val t = tokens[i]
                if (t[0].isLetter()) {
                    cmd = t[0]
                    i++
                    if (cmd == 'z' || cmd == 'Z') {
                        path.close()
                        continue
                    }
                }
                when (cmd) {
                    'M' -> { x = num(); y = num(); path.moveTo(x, y); cmd = 'L' }
                    'L' -> { x = num(); y = num(); path.lineTo(x, y) }
                    'l' -> { x += num(); y += num(); path.lineTo(x, y) }
                    'H' -> { x = num(); path.lineTo(x, y) }
                    'h' -> { x += num(); path.lineTo(x, y) }
                    'V' -> { y = num(); path.lineTo(x, y) }
                    'v' -> { y += num(); path.lineTo(x, y) }
                    'a' -> {
                        repeat(5) { num() }
                        num()
                        num()
                        path.addCircle(x, y, 1.5f, Path.Direction.CW)
                    }
                    else -> i++
                }
            }
            return path
        }
    }
}
