from __future__ import annotations

import copy
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path


SVG_NS = "http://www.w3.org/2000/svg"
XLINK_NS = "http://www.w3.org/1999/xlink"
ET.register_namespace("", SVG_NS)
ET.register_namespace("xlink", XLINK_NS)

BRAND_DIR = Path(__file__).resolve().parent
REPO_DIR = BRAND_DIR.parents[1]
GENERATED_DIR = BRAND_DIR / "generated"
VECTOR_DIR = GENERATED_DIR / "vector-src"
WEB_PUBLIC_DIR = REPO_DIR / "web" / "public"
DOCS_PUBLIC_DIR = REPO_DIR / "docs-site" / "docs" / "public"
ICON_SOURCE = BRAND_DIR / "dockrev-icon-source.svg"
LOGO_SOURCE = BRAND_DIR / "dockrev-logo-source.svg"
WORDMARK_SOURCE = VECTOR_DIR / "dockrev-text-pango.svg"
SOCIAL_PREVIEW_SOURCE = GENERATED_DIR / "dockrev-github-social-preview-imagegen-candidate.png"
PRODUCT_POSTER_DARK_SOURCE = GENERATED_DIR / "dockrev-product-poster-dark.png"
PRODUCT_POSTER_LIGHT_SOURCE = GENERATED_DIR / "dockrev-product-poster-light.png"
SOCIAL_FONT = REPO_DIR / "crates" / "dockrev-api" / "assets" / "fonts" / "NotoSansCJKsc-Regular.otf"
PNG_DETERMINISTIC_OPTIONS = ["-strip", "-define", "png:exclude-chunk=tIME"]

DARK_CYAN = ("#20b8ff", "#1eb6fe", "#1cb4fb")
DARK_GREEN = ("#16d563", "#10cd5c")
LIGHT_CYAN = ("#0d86dd", "#0c79cf", "#086cba")
LIGHT_GREEN = ("#16934e", "#138347")


def svg_tag(name: str) -> str:
    return f"{{{SVG_NS}}}{name}"


def read_svg(path: Path) -> ET.Element:
    return ET.parse(path).getroot()


def write_svg(root: ET.Element, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    ET.indent(root, space="  ")
    ET.ElementTree(root).write(path, encoding="utf-8", xml_declaration=True)


def remove_background(root: ET.Element) -> None:
    for child in list(root):
        if child.get("id") == "icon-background":
            root.remove(child)
            return


def recolor_icon(root: ET.Element, theme: str) -> None:
    cyan = DARK_CYAN if theme == "dark" else LIGHT_CYAN
    green = DARK_GREEN if theme == "dark" else LIGHT_GREEN
    for gradient in root.iter(svg_tag("linearGradient")):
        colors = cyan if gradient.get("id") == "cyan" else green
        for stop, color in zip(gradient.findall(svg_tag("stop")), colors, strict=True):
            stop.set("stop-color", color)


def icon_variant(theme: str, include_background: bool) -> ET.Element:
    root = read_svg(ICON_SOURCE)
    recolor_icon(root, theme)
    if not include_background:
        remove_background(root)
    return root


def maskable_icon_variant() -> ET.Element:
    """Place the dark product mark in a platform-owned, opaque application canvas."""
    source = icon_variant("dark", include_background=False)
    root = ET.Element(
        svg_tag("svg"),
        {
            "viewBox": "0 0 1024 1024",
            "role": "img",
            "aria-labelledby": "title desc",
        },
    )
    title = ET.SubElement(root, svg_tag("title"), {"id": "title"})
    title.text = "Dockrev maskable icon"
    desc = ET.SubElement(root, svg_tag("desc"), {"id": "desc"})
    desc.text = "Dockrev product mark inside a platform-safe opaque application canvas."
    ET.SubElement(root, svg_tag("rect"), {"width": "1024", "height": "1024", "fill": "#010e2d"})

    # The transparent mark's real alpha bounds are 777x741 on its 1024 canvas.
    # This transform centers a 60% max-edge foreground without baking system chrome.
    mark = ET.SubElement(root, svg_tag("g"), {"transform": "translate(91 110) scale(0.7906)"})
    for child in list(source):
        if child.tag not in {svg_tag("title"), svg_tag("desc")}:
            mark.append(copy.deepcopy(child))
    return root


def logo_variant(theme: str) -> ET.Element:
    root = ET.Element(
        svg_tag("svg"),
        {
            "viewBox": "0 0 1280 365",
            "role": "img",
            "aria-labelledby": "logo-title logo-desc",
        },
    )
    title = ET.SubElement(root, svg_tag("title"), {"id": "logo-title"})
    title.text = "Dockrev"
    desc = ET.SubElement(root, svg_tag("desc"), {"id": "logo-desc"})
    desc.text = "Dockrev update manager logo"

    defs = ET.SubElement(root, svg_tag("defs"))
    wordmark_root = read_svg(WORDMARK_SOURCE)
    wordmark_defs = wordmark_root.find(svg_tag("defs"))
    if wordmark_defs is None:
        raise RuntimeError(f"Missing wordmark definitions in {WORDMARK_SOURCE}")
    for child in list(wordmark_defs):
        defs.append(copy.deepcopy(child))

    mark = icon_variant(theme, include_background=False)
    mark.attrib.clear()
    mark.attrib.update(
        {
            "x": "0",
            "y": "0",
            "width": "350",
            "height": "365",
            "viewBox": "120 110 820 800",
            "preserveAspectRatio": "xMidYMid meet",
            "aria-hidden": "true",
        }
    )
    for child in list(mark):
        if child.tag in {svg_tag("title"), svg_tag("desc")}:
            mark.remove(child)
    root.append(mark)

    wordmark_group = next(
        (child for child in list(wordmark_root) if child.tag == svg_tag("g")),
        None,
    )
    if wordmark_group is None:
        raise RuntimeError(f"Missing wordmark glyph group in {WORDMARK_SOURCE}")
    wordmark = copy.deepcopy(wordmark_group)
    wordmark.set("fill", "#e8f1ff" if theme == "dark" else "#102842")
    wordmark.set("transform", "translate(408 68) scale(1.22)")
    wordmark.set("aria-hidden", "true")
    root.append(wordmark)
    return root


def render_svg(svg_path: Path, output_path: Path, width: int, height: int) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            "rsvg-convert",
            "-w",
            str(width),
            "-h",
            str(height),
            str(svg_path),
            "-o",
            str(output_path),
        ],
        check=True,
    )


def copy_to(path: Path, destinations: list[Path]) -> None:
    for destination in destinations:
        destination.parent.mkdir(parents=True, exist_ok=True)
        if path.resolve() == destination.resolve():
            continue
        shutil.copyfile(path, destination)


def require_aspect_ratio(path: Path, width_ratio: int, height_ratio: int) -> tuple[int, int]:
    if not path.exists():
        raise RuntimeError(f"Missing brand media source: {path}")

    result = subprocess.run(
        ["magick", "identify", "-format", "%w %h", str(path)],
        check=True,
        capture_output=True,
        text=True,
    )
    try:
        width, height = (int(value) for value in result.stdout.split())
    except ValueError as error:
        raise RuntimeError(f"Could not read image dimensions from {path}") from error

    if width * height_ratio != height * width_ratio:
        raise RuntimeError(
            f"Expected {path} to have a {width_ratio}:{height_ratio} aspect ratio, "
            f"received {width}x{height}"
        )
    return width, height


def generate_svg_assets() -> tuple[Path, Path, Path, Path]:
    icon_dark = GENERATED_DIR / "dockrev-icon-dark.svg"
    icon_light = GENERATED_DIR / "dockrev-icon-light.svg"
    icon_square = GENERATED_DIR / "dockrev-icon-square.svg"
    icon_maskable = GENERATED_DIR / "dockrev-icon-maskable.svg"
    logo_dark = GENERATED_DIR / "dockrev-logo-dark.svg"
    logo_light = GENERATED_DIR / "dockrev-logo-light.svg"

    write_svg(icon_variant("dark", include_background=False), icon_dark)
    write_svg(icon_variant("light", include_background=False), icon_light)
    write_svg(icon_variant("dark", include_background=True), icon_square)
    write_svg(maskable_icon_variant(), icon_maskable)
    write_svg(logo_variant("dark"), LOGO_SOURCE)
    write_svg(logo_variant("dark"), logo_dark)
    write_svg(logo_variant("light"), logo_light)

    copy_to(
        icon_dark,
        [
            GENERATED_DIR / "dockrev-icon.svg",
            GENERATED_DIR / "dockrev-icon-clean-trace.svg",
            VECTOR_DIR / "dockrev-clean-icon-symbol.svg",
            DOCS_PUBLIC_DIR / "dockrev-icon.svg",
        ],
    )
    copy_to(
        icon_square,
        [
            WEB_PUBLIC_DIR / "favicon.svg",
            WEB_PUBLIC_DIR / "favicon-dark.svg",
            WEB_PUBLIC_DIR / "favicon-light.svg",
            DOCS_PUBLIC_DIR / "favicon.svg",
            DOCS_PUBLIC_DIR / "favicon-dark.svg",
            DOCS_PUBLIC_DIR / "favicon-light.svg",
        ],
    )
    copy_to(
        logo_dark,
        [
            GENERATED_DIR / "dockrev-logo.svg",
            GENERATED_DIR / "dockrev-logo-clean-trace.svg",
            WEB_PUBLIC_DIR / "dockrev-logo.svg",
            WEB_PUBLIC_DIR / "dockrev-logo-dark.svg",
            DOCS_PUBLIC_DIR / "dockrev-logo.svg",
            DOCS_PUBLIC_DIR / "dockrev-logo-dark.svg",
        ],
    )
    copy_to(
        logo_light,
        [
            WEB_PUBLIC_DIR / "dockrev-logo-light.svg",
            DOCS_PUBLIC_DIR / "dockrev-logo-light.svg",
        ],
    )
    return icon_dark, icon_square, icon_maskable, logo_dark


def generate_raster_assets(icon_dark: Path, icon_square: Path, icon_maskable: Path, logo_dark: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="dockrev-brand-") as temp_dir:
        temp = Path(temp_dir)
        icon_128 = temp / "icon-128.png"
        icon_180 = temp / "icon-180.png"
        icon_192 = temp / "icon-192.png"
        maskable_icon_180 = temp / "maskable-icon-180.png"
        maskable_icon_192 = temp / "maskable-icon-192.png"
        maskable_icon_512 = temp / "maskable-icon-512.png"
        icon_256 = temp / "icon-256.png"
        icon_512 = temp / "icon-512.png"
        icon_1024_transparent = temp / "icon-transparent-1024.png"
        icon_2048 = temp / "icon-2048.png"
        logo_1280 = temp / "logo-1280.png"
        logo_candidate = temp / "logo-candidate.png"
        favicon_ico = temp / "favicon.ico"

        render_svg(icon_square, icon_128, 128, 128)
        render_svg(icon_square, icon_180, 180, 180)
        render_svg(icon_square, icon_192, 192, 192)
        render_svg(icon_square, icon_256, 256, 256)
        render_svg(icon_square, icon_512, 512, 512)
        render_svg(icon_maskable, maskable_icon_180, 180, 180)
        render_svg(icon_maskable, maskable_icon_192, 192, 192)
        render_svg(icon_maskable, maskable_icon_512, 512, 512)
        render_svg(icon_dark, icon_1024_transparent, 1024, 1024)
        render_svg(icon_square, icon_2048, 2048, 2048)
        render_svg(logo_dark, logo_1280, 1280, 365)

        subprocess.run(
            [
                "magick",
                str(icon_256),
                "-define",
                "icon:auto-resize=256,128,64,48,32,16",
                str(favicon_ico),
            ],
            check=True,
        )
        subprocess.run(
            [
                "magick",
                "-size",
                "2048x1024",
                "xc:#010e2d",
                "(",
                str(logo_1280),
                "-resize",
                "1680x479",
                ")",
                "-gravity",
                "center",
                "-composite",
                "-depth",
                "8",
                *PNG_DETERMINISTIC_OPTIONS,
                str(logo_candidate),
            ],
            check=True,
        )

        copy_to(
            icon_2048,
            [
                BRAND_DIR / "dockrev-icon-source.png",
                GENERATED_DIR / "dockrev-icon-candidate.png",
            ],
        )
        copy_to(logo_1280, [BRAND_DIR / "dockrev-logo-source.png"])
        copy_to(logo_candidate, [GENERATED_DIR / "dockrev-logo-candidate.png"])
        copy_to(
            icon_1024_transparent,
            [
                GENERATED_DIR / "dockrev-icon-transparent.png",
                GENERATED_DIR / "dockrev-icon-vector-source.png",
                VECTOR_DIR / "dockrev-icon-clean-flat.png",
            ],
        )
        copy_to(
            logo_1280,
            [
                GENERATED_DIR / "dockrev-logo.png",
                GENERATED_DIR / "dockrev-logo-transparent.png",
                GENERATED_DIR / "dockrev-logo-vector-source.png",
                VECTOR_DIR / "dockrev-logo-clean-flat.png",
                WEB_PUBLIC_DIR / "dockrev-logo.png",
                DOCS_PUBLIC_DIR / "dockrev-logo.png",
            ],
        )
        copy_to(
            icon_128,
            [
                WEB_PUBLIC_DIR / "brand-mark.png",
                DOCS_PUBLIC_DIR / "dockrev-icon.png",
            ],
        )
        # The docs site keeps its own Apple touch icon; the product app uses Manifest metadata only.
        copy_to(maskable_icon_180, [DOCS_PUBLIC_DIR / "apple-touch-icon.png"])
        copy_to(icon_192, [WEB_PUBLIC_DIR / "pwa-192.png"])
        copy_to(icon_256, [WEB_PUBLIC_DIR / "favicon.png", DOCS_PUBLIC_DIR / "favicon.png"])
        copy_to(icon_512, [WEB_PUBLIC_DIR / "pwa-512.png"])
        copy_to(maskable_icon_192, [WEB_PUBLIC_DIR / "pwa-maskable-192.png"])
        copy_to(maskable_icon_512, [WEB_PUBLIC_DIR / "pwa-maskable-512.png"])
        copy_to(favicon_ico, [WEB_PUBLIC_DIR / "favicon.ico", DOCS_PUBLIC_DIR / "favicon.ico"])


def generate_social_preview() -> None:
    with tempfile.TemporaryDirectory(prefix="dockrev-social-") as temp_dir:
        temp = Path(temp_dir)
        source = temp / "source.png"
        product = temp / "product.png"
        product_mask = temp / "product-mask.png"
        product_faded = temp / "product-faded.png"
        logo = temp / "logo.png"
        preview = temp / "dockrev-social-preview.png"

        subprocess.run(
            ["magick", str(SOCIAL_PREVIEW_SOURCE), "-resize", "1280x640!", str(source)],
            check=True,
        )
        subprocess.run(
            ["magick", str(source), "-crop", "760x640+520+0", "+repage", str(product)],
            check=True,
        )
        subprocess.run(
            [
                "magick",
                "-size",
                "760x640",
                "xc:black",
                "-channel",
                "R",
                "-fx",
                "i < 220 ? i / 220 : 1",
                "-separate",
                str(product_mask),
            ],
            check=True,
        )
        subprocess.run(
            [
                "magick",
                str(product),
                str(product_mask),
                "-alpha",
                "off",
                "-compose",
                "CopyOpacity",
                "-composite",
                str(product_faded),
            ],
            check=True,
        )
        subprocess.run(
            ["magick", str(WEB_PUBLIC_DIR / "dockrev-logo.png"), "-resize", "490x140", str(logo)],
            check=True,
        )
        subprocess.run(
            [
                "magick",
                "-size",
                "1280x640",
                "xc:#010e2d",
                "-stroke",
                "#0b4b78",
                "-strokewidth",
                "1",
                "-fill",
                "none",
                "-draw",
                "polyline 0,102 158,102 190,126 382,126 polyline 0,526 206,526 240,500 420,500",
                str(product_faded),
                "-geometry",
                "+520+0",
                "-compose",
                "over",
                "-composite",
                str(logo),
                "-geometry",
                "+28+184",
                "-composite",
                "-font",
                str(SOCIAL_FONT),
                "-stroke",
                "none",
                "-fill",
                "#e8f1ff",
                "-pointsize",
                "32",
                "-interline-spacing",
                "8",
                "-annotate",
                "+50+392",
                "Self-hosted Docker/Compose\nupdate manager",
                "-depth",
                "8",
                *PNG_DETERMINISTIC_OPTIONS,
                str(preview),
            ],
            check=True,
        )
        copy_to(
            preview,
            [
                GENERATED_DIR / "dockrev-github-social-preview.png",
                WEB_PUBLIC_DIR / "dockrev-social-preview.png",
                DOCS_PUBLIC_DIR / "dockrev-social-preview.png",
            ],
        )


def generate_product_poster() -> None:
    require_aspect_ratio(PRODUCT_POSTER_DARK_SOURCE, 4, 5)
    subprocess.run(
        [
            sys.executable,
            str(BRAND_DIR / "recolor_product_poster.py"),
            str(PRODUCT_POSTER_DARK_SOURCE),
            str(PRODUCT_POSTER_LIGHT_SOURCE),
        ],
        check=True,
    )
    poster_variants = (
        ("dark", PRODUCT_POSTER_DARK_SOURCE),
        ("light", PRODUCT_POSTER_LIGHT_SOURCE),
    )
    for theme, source in poster_variants:
        require_aspect_ratio(source, 4, 5)
        copy_to(
            source,
            [
                GENERATED_DIR / f"dockrev-product-poster-{theme}.png",
                WEB_PUBLIC_DIR / f"dockrev-product-poster-{theme}.png",
                DOCS_PUBLIC_DIR / f"dockrev-product-poster-{theme}.png",
            ],
        )

    # Keep existing unqualified consumers on the dark variant until the full
    # light/dark media matrix has a documented selection policy.
    copy_to(
        PRODUCT_POSTER_DARK_SOURCE,
        [
            GENERATED_DIR / "dockrev-product-poster.png",
            WEB_PUBLIC_DIR / "dockrev-product-poster.png",
            DOCS_PUBLIC_DIR / "dockrev-product-poster.png",
        ],
    )


def generate_contact_sheet() -> None:
    with tempfile.TemporaryDirectory(prefix="dockrev-contact-sheet-") as temp_dir:
        temp = Path(temp_dir)
        favicon = temp / "favicon.png"
        brand_mark = temp / "brand-mark.png"
        logo = temp / "logo.png"
        social = temp / "social.png"
        poster = temp / "poster.png"
        sheet = temp / "contact-sheet.png"

        resize_specs = (
            (WEB_PUBLIC_DIR / "favicon.png", favicon, "320x320"),
            (WEB_PUBLIC_DIR / "brand-mark.png", brand_mark, "180x180"),
            (WEB_PUBLIC_DIR / "dockrev-logo.png", logo, "560x160"),
            (WEB_PUBLIC_DIR / "dockrev-social-preview.png", social, "720x360"),
            (WEB_PUBLIC_DIR / "dockrev-product-poster-dark.png", poster, "420x525"),
        )
        for source, destination, size in resize_specs:
            subprocess.run(["magick", str(source), "-resize", size, str(destination)], check=True)

        subprocess.run(
            [
                "magick",
                "-size",
                "1600x1500",
                "xc:#010e2d",
                "-font",
                str(SOCIAL_FONT),
                "-fill",
                "#e8f1ff",
                "-pointsize",
                "46",
                "-annotate",
                "+48+72",
                "Dockrev final brand assets",
                "-fill",
                "none",
                "-stroke",
                "#20b8ff",
                "-strokewidth",
                "2",
                "-draw",
                "roundrectangle 48,120 448,570 8,8 roundrectangle 472,120 848,570 8,8 roundrectangle 872,120 1548,570 8,8 roundrectangle 48,620 824,1050 8,8 roundrectangle 872,620 1548,1400 8,8",
                str(favicon),
                "-geometry",
                "+88+160",
                "-compose",
                "over",
                "-composite",
                str(brand_mark),
                "-geometry",
                "+570+220",
                "-composite",
                str(logo),
                "-geometry",
                "+930+220",
                "-composite",
                str(social),
                "-geometry",
                "+76+650",
                "-composite",
                str(poster),
                "-geometry",
                "+1000+725",
                "-composite",
                "-stroke",
                "none",
                "-fill",
                "#b9cee5",
                "-pointsize",
                "25",
                "-annotate",
                "+72+548",
                "Favicon 256x256",
                "-annotate",
                "+496+548",
                "Brand mark 128x128",
                "-annotate",
                "+896+548",
                "Horizontal logo 1280x365",
                "-annotate",
                "+72+1028",
                "Social preview 1280x640",
                "-annotate",
                "+896+1375",
                "Product poster 4:5",
                "-depth",
                "8",
                *PNG_DETERMINISTIC_OPTIONS,
                str(sheet),
            ],
            check=True,
        )
        copy_to(
            sheet,
            [
                GENERATED_DIR / "final-site-assets-contact-sheet.png",
                GENERATED_DIR / "visual-evidence-site-assets.png",
            ],
        )


def main() -> None:
    required_commands = ("rsvg-convert", "magick")
    missing = [command for command in required_commands if shutil.which(command) is None]
    if missing:
        raise RuntimeError(f"Missing required commands: {', '.join(missing)}")
    if not SOCIAL_FONT.exists():
        raise RuntimeError(f"Missing bundled social preview font: {SOCIAL_FONT}")
    icon_dark, icon_square, icon_maskable, logo_dark = generate_svg_assets()
    generate_raster_assets(icon_dark, icon_square, icon_maskable, logo_dark)
    generate_social_preview()
    generate_product_poster()
    generate_contact_sheet()
    print("Generated Dockrev brand assets from canonical SVG sources.")


if __name__ == "__main__":
    main()
