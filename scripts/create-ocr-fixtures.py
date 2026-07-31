from pathlib import Path
import sys

from PIL import Image, ImageDraw, ImageFont


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: create-ocr-fixtures.py <output-directory>")
    output = Path(sys.argv[1]).resolve()
    output.mkdir(parents=True, exist_ok=True)
    image = Image.new("RGB", (1200, 260), "white")
    draw = ImageDraw.Draw(image)
    font = ImageFont.truetype("C:/Windows/Fonts/arial.ttf", 60)
    draw.text((45, 85), "everyfile ocr acceptance 8427", fill="black", font=font)
    for extension, format_name in [
        ("jpg", "JPEG"),
        ("png", "PNG"),
        ("webp", "WEBP"),
        ("bmp", "BMP"),
        ("tiff", "TIFF"),
    ]:
        image.save(output / f"ocr-acceptance.{extension}", format=format_name)
    image.save(output / "ocr-scanned.pdf", format="PDF", resolution=150)


if __name__ == "__main__":
    main()
