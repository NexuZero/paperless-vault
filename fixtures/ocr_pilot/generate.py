#!/usr/bin/env python3
"""Synthetic OCR-pilot fixtures for T-108. Never real documents (CLAUDE.md §5).

Each image embeds a known 'check phrase' so the OCR spike can verify it was actually
recognised, not just that Tesseract ran without crashing.
"""
import pathlib

from PIL import Image, ImageDraw, ImageFont

OUT = pathlib.Path(__file__).parent
ARABIC_FONT = "/usr/share/fonts/truetype/noto/NotoNaskhArabic-Regular.ttf"
LATIN_FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"

# (filename, lines: list of (text, font_path, size, direction), check_phrase)
DOCS = [
    (
        "01_english_invoice.png",
        [
            ("INVOICE #A-2026-0913", LATIN_FONT, 44, "ltr"),
            ("Paperless Vault Test Fixtures Ltd.", LATIN_FONT, 32, "ltr"),
            ("Bill to: Zik — Test Account", LATIN_FONT, 32, "ltr"),
            ("Check phrase: quantum lighthouse cascade", LATIN_FONT, 36, "ltr"),
            ("Total due: $128.40", LATIN_FONT, 32, "ltr"),
        ],
        "quantum lighthouse cascade",
    ),
    (
        "02_english_certificate.png",
        [
            ("CERTIFICATE OF COMPLETION", LATIN_FONT, 46, "ltr"),
            ("This certifies that the bearer has completed", LATIN_FONT, 30, "ltr"),
            ("the synthetic fixture generation course.", LATIN_FONT, 30, "ltr"),
            ("Check phrase: amber orchard velocity", LATIN_FONT, 36, "ltr"),
        ],
        "amber orchard velocity",
    ),
    (
        "03_arabic_invoice.png",
        [
            ("فاتورة رقم ٢٠٢٦/٠٩/١٣", ARABIC_FONT, 44, "rtl"),
            ("شركة بيانات الاختبار المحدودة", ARABIC_FONT, 34, "rtl"),
            ("عبارة التحقق: نجم بحيرة صامتة", ARABIC_FONT, 38, "rtl"),
            ("المبلغ الإجمالي: ١٢٨٫٤٠ دولار", ARABIC_FONT, 34, "rtl"),
        ],
        "نجم بحيرة صامتة",
    ),
    (
        "04_arabic_certificate.png",
        [
            ("شهادة إتمام", ARABIC_FONT, 48, "rtl"),
            ("تشهد هذه الوثيقة بأن حامل الشهادة قد أتم", ARABIC_FONT, 30, "rtl"),
            ("دورة إنشاء بيانات الاختبار التركيبية", ARABIC_FONT, 30, "rtl"),
            ("عبارة التحقق: غابة الفجر الهادئة", ARABIC_FONT, 36, "rtl"),
        ],
        "غابة الفجر الهادئة",
    ),
    (
        "05_mixed_bilingual_notice.png",
        [
            ("BILINGUAL NOTICE / إشعار ثنائي اللغة", LATIN_FONT, 38, "ltr"),
            ("Document type: Official Notice", LATIN_FONT, 30, "ltr"),
            ("نوع الوثيقة: إشعار رسمي", ARABIC_FONT, 30, "rtl"),
            ("Check phrase: coral summit archive", LATIN_FONT, 34, "ltr"),
            ("عبارة التحقق الثانية: نسيم الوادي الأزرق", ARABIC_FONT, 32, "rtl"),
        ],
        "coral summit archive",
    ),
]


def render(filename, lines, check_phrase):
    img = Image.new("L", (1600, 1000), color=255)
    draw = ImageDraw.Draw(img)
    y = 80
    for text, font_path, size, direction in lines:
        font = ImageFont.truetype(font_path, size)
        draw.text((100, y), text, fill=0, font=font, direction=direction)
        y += size + 40
    path = OUT / filename
    img.save(path)
    print(f"{filename}\tcheck_phrase={check_phrase!r}")


if __name__ == "__main__":
    manifest = OUT / "MANIFEST.tsv"
    with open(manifest, "w") as mf:
        mf.write("filename\tcheck_phrase\tlanguage\n")
        for filename, lines, check_phrase in DOCS:
            render(filename, lines, check_phrase)
            lang = "ara" if filename.startswith(("03", "04")) else ("mixed" if filename.startswith("05") else "eng")
            mf.write(f"{filename}\t{check_phrase}\t{lang}\n")
    print(f"wrote {manifest}")
