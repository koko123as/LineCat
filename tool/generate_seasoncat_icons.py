#!/usr/bin/env python3
"""Generate every SeasonCat app icon from assets_source/images/icon/seasoncat.jpg.

The source is a full-bleed 1024x1024 square: a smooth gradient with a black
cat. The script splits it into a gradient plate (a polynomial fitted to the
background, so it extends past the edges without seams) and a cat layer
(alpha from the dark strokes), then renders each platform's files. Corners
outside a rounded shape are transparent, never black.

Requires Pillow and numpy:  python3 tool/generate_seasoncat_icons.py
"""
import io
import os
from PIL import Image, ImageDraw, ImageFilter
import numpy as np

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, "assets_source/images/icon/seasoncat.jpg")
RES = os.path.join(ROOT, "android/app/src/main/res")
SERVICE_RES = os.path.join(ROOT, "android/service/src/main/res")
MAC = os.path.join(ROOT, "macos/Runner/Assets.xcassets/AppIcon.appiconset")
SS = 4  # supersampling for masks


def p(*parts):
    return os.path.join(ROOT, *parts)


src = np.asarray(Image.open(SRC).convert("RGB")).astype(np.float64) / 255
N = src.shape[0]
lum = src @ [0.299, 0.587, 0.114]
cat_a = np.clip((0.40 - lum) / 0.25, 0, 1)
exclude = np.asarray(
    Image.fromarray(((cat_a > 0.02) * 255).astype(np.uint8)).filter(ImageFilter.MaxFilter(31))
) > 0


def _basis(u, v, d=4):
    return np.stack([u**i * v**j for i in range(d + 1) for j in range(d + 1 - i)], -1)


ys, xs = np.mgrid[0:N, 0:N]
_u, _v = xs / (N - 1) * 2 - 1, ys / (N - 1) * 2 - 1
_idx = np.where((~exclude).ravel())[0][::5]
_B = _basis(_u, _v).reshape(-1, 15)[_idx]
COEF = np.array([np.linalg.lstsq(_B, src[..., c].ravel()[_idx], rcond=None)[0] for c in range(3)])

CAT = Image.fromarray((cat_a * 255).astype(np.uint8))  # full-image coordinates


def plate(w, h, x0=-1.0, x1=1.0, y0=-1.0, y1=1.0):
    """Gradient over source coords [x0,x1]x[y0,y1]; outside [-1,1] edge-extended."""
    u = np.clip(np.linspace(x0, x1, w), -1, 1)[None, :].repeat(h, 0)
    v = np.clip(np.linspace(y0, y1, h), -1, 1)[:, None].repeat(w, 1)
    b = _basis(u, v)
    rgb = np.clip(np.stack([b @ COEF[c] for c in range(3)], -1), 0, 1)
    return Image.fromarray((rgb * 255).round().astype(np.uint8), "RGB")


def cat_layer(size, color=(0, 0, 0)):
    """The cat on transparency, the source square mapped onto size x size."""
    a = CAT.resize((size, size), Image.LANCZOS)
    layer = Image.new("RGBA", (size, size), color + (0,))
    layer.putalpha(a)
    return layer


def full(size):
    """Gradient + cat, full bleed, opaque."""
    img = plate(size, size).convert("RGBA")
    img.alpha_composite(cat_layer(size))
    return img


def masked(img, shape, radius=0.2237):
    s = img.size[0]
    m = Image.new("L", (s * SS, s * SS), 0)
    d = ImageDraw.Draw(m)
    if shape == "circle":
        d.ellipse((0, 0, s * SS - 1, s * SS - 1), fill=255)
    else:
        d.rounded_rectangle((0, 0, s * SS - 1, s * SS - 1), radius=radius * s * SS, fill=255)
    m = m.resize((s, s), Image.LANCZOS)
    out = img.copy()
    out.putalpha(Image.fromarray(np.minimum(np.asarray(out.getchannel("A")), np.asarray(m))))
    return out


def save(img, path, **kw):
    path = p(path) if not os.path.isabs(path) else path
    os.makedirs(os.path.dirname(path), exist_ok=True)
    if path.endswith(".webp"):
        img.save(path, "WEBP", lossless=True, quality=100, method=6)
    else:
        img.save(path, **kw)
    print("generated", os.path.relpath(path, ROOT))


big = full(1024)

# Desktop and README icon: rounded square, transparent corners.
save(masked(big, "round"), "assets/images/icon.png")
ico_sizes = [16, 24, 32, 48, 64, 128, 256]
ico = masked(big, "round")
for path in ["assets/images/icon.ico", "windows/runner/resources/app_icon.ico"]:
    save(ico, path, format="ICO", sizes=[(s, s) for s in ico_sizes])

# macOS: 824/1024 plate on the standard grid, squircle-ish radius.
for s in [16, 32, 64, 128, 256, 512, 1024]:
    canvas = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    inner = max(1, round(s * 824 / 1024))
    tile = masked(full(inner * 2).resize((inner, inner), Image.LANCZOS), "round", 0.225)
    off = (s - inner) // 2
    canvas.alpha_composite(tile, (off, off))
    save(canvas, f"{MAC}/app_icon_{s}.png")

# Android adaptive layers (108dp canvas, xxxhdpi = 4x => 432px plate).
# Visible 72dp shows exactly the source square: plate spans [-1.5, 1.5].
save(plate(432, 432, -1.5, 1.5, -1.5, 1.5), f"{RES}/drawable-xxxhdpi/ic_launcher_plate.webp")
# Foreground art is the source square; ic_launcher_foreground insets it 16.667% (18dp of 108).
save(cat_layer(720), f"{RES}/drawable-xxxhdpi/ic_launcher_art.webp")

# Legacy launcher bitmaps (API < 26).
for name, px in {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}.items():
    base = big.resize((px, px), Image.LANCZOS)
    save(masked(base, "round", 0.2237), f"{RES}/mipmap-{name}/ic_launcher.webp")
    save(masked(base, "circle"), f"{RES}/mipmap-{name}/ic_launcher_round.webp")

# Android TV launcher bitmaps (opaque squares).
for name, px in {"mdpi": 80, "hdpi": 120, "xhdpi": 160, "xxhdpi": 240, "xxxhdpi": 320}.items():
    save(big.resize((px, px), Image.LANCZOS).convert("RGB"), f"{RES}/mipmap-television-{name}/ic_launcher.webp")

# TV banner 320x180: gradient across, cat centred.
bw, bh = 320 * 4, 180 * 4
banner = plate(bw, bh, -1, 1, -180 / 320, 180 / 320).convert("RGBA")
c = cat_layer(bh * 1024 // 640)  # cat ~ (478/1024) of height*1.6
banner.alpha_composite(c, ((bw - c.size[0]) // 2, (bh - c.size[1]) // 2))
save(banner.resize((320, 180), Image.LANCZOS).convert("RGB"), f"{RES}/mipmap-xhdpi/ic_banner.png")

# Play Store 512, opaque full bleed (Play applies its own mask).
save(big.resize((512, 512), Image.LANCZOS).convert("RGB"), "android/app/src/main/ic_launcher-playstore.png")

# Notification / documents-provider small icon: white cat, 24dp, cropped to the glyph.
bbox = CAT.point(lambda v: 255 if v > 8 else 0).getbbox()
glyph = CAT.crop(bbox)
gw, gh = glyph.size
side = max(gw, gh)
sq = Image.new("L", (side, side), 0)
sq.paste(glyph, ((side - gw) // 2, (side - gh) // 2))
for name, px in {"mdpi": 24, "hdpi": 36, "xhdpi": 48, "xxhdpi": 72, "xxxhdpi": 96}.items():
    inner = round(px * 22 / 24)
    a = Image.new("L", (px, px), 0)
    a.paste(sq.resize((inner, inner), Image.LANCZOS), ((px - inner) // 2, (px - inner) // 2))
    icon = Image.new("RGBA", (px, px), (255, 255, 255, 0))
    icon.putalpha(a)
    save(icon, f"{SERVICE_RES}/drawable-{name}/ic_service.png")
