"""Draws the disk image background: red header strip, light canvas and an arrow."""
import sys

from PIL import Image, ImageDraw, ImageFont

SCALE = int(sys.argv[2]) if len(sys.argv) > 2 else 1
W, H = 640, 400
image = Image.new("RGB", (W * SCALE, H * SCALE), (236, 236, 236))
draw = ImageDraw.Draw(image)
draw.rectangle((0, 0, W * SCALE, 64 * SCALE), fill=(196, 22, 28))
draw.rectangle((0, 61 * SCALE, W * SCALE, 64 * SCALE), fill=(158, 15, 20))


def font(size):
    for path in ("/System/Library/Fonts/SFNS.ttf", "/System/Library/Fonts/Helvetica.ttc"):
        try:
            return ImageFont.truetype(path, size * SCALE)
        except OSError:
            pass
    return ImageFont.load_default()


draw.text((28 * SCALE, 18 * SCALE), "SSD Scope for Mac", font=font(22), fill="white")
draw.text(
    (W * SCALE // 2, 330 * SCALE),
    "Drag SSD Scope to Applications to install",
    font=font(14),
    fill=(90, 90, 90),
    anchor="mm",
)
# Arrow between the two icons.
y = 205 * SCALE
draw.line((250 * SCALE, y, 380 * SCALE, y), fill=(196, 22, 28), width=6 * SCALE)
draw.polygon(
    [(392 * SCALE, y), (372 * SCALE, y - 14 * SCALE), (372 * SCALE, y + 14 * SCALE)],
    fill=(196, 22, 28),
)
image.save(sys.argv[1], dpi=(72 * SCALE, 72 * SCALE))
