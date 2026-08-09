from __future__ import annotations

import copy
import shutil
import subprocess
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
SOCIAL_FONT = REPO_DIR / "crates" / "dockrev-api" / "assets" / "fonts" / "NotoSansCJKsc-Regular.otf"

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
        shutil.copyfile(path, destination)


def generate_svg_assets() -> tuple[Path, Path, Path]:
    icon_dark = GENERATED_DIR / "dockrev-icon-dark.svg"
    icon_light = GENERATED_DIR / "dockrev-icon-light.svg"
    icon_square = GENERATED_DIR / "dockrev-icon-square.svg"
    logo_dark = GENERATED_DIR / "dockrev-logo-dark.svg"
    logo_light = GENERATED_DIR / "dockrev-logo-light.svg"

    write_svg(icon_variant("dark", include_background=False), icon_dark)
    write_svg(icon_variant("light", include_background=False), icon_light)
    write_svg(icon_variant("dark", include_background=True), icon_square)
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
    return icon_dark, icon_square, logo_dark


def generate_raster_assets(icon_dark: Path, icon_square: Path, logo_dark: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="dockrev-brand-") as temp_dir:
        temp = Path(temp_dir)
        icon_128 = temp / "icon-128.png"
        icon_180 = temp / "icon-180.png"
        icon_192 = temp / "icon-192.png"
        icon_256 = temp / "icon-256.png"
        icon_512 = temp / "icon-512.png"
        icon_1024_transparent = temp / "icon-transparent-1024.png"
        icon_2048 = temp / "icon-2048.png"
        logo_1280 = temp / "logo-1280.png"
        favicon_ico = temp / "favicon.ico"

        render_svg(icon_square, icon_128, 128, 128)
        render_svg(icon_square, icon_180, 180, 180)
        render_svg(icon_square, icon_192, 192, 192)
        render_svg(icon_square, icon_256, 256, 256)
        render_svg(icon_square, icon_512, 512, 512)
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

        copy_to(icon_2048, [BRAND_DIR / "dockrev-icon-source.png"])
        copy_to(logo_1280, [BRAND_DIR / "dockrev-logo-source.png"])
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
        copy_to(icon_180, [WEB_PUBLIC_DIR / "apple-touch-icon.png", DOCS_PUBLIC_DIR / "apple-touch-icon.png"])
        copy_to(icon_192, [WEB_PUBLIC_DIR / "pwa-192.png"])
        copy_to(icon_256, [WEB_PUBLIC_DIR / "favicon.png", DOCS_PUBLIC_DIR / "favicon.png"])
        copy_to(icon_512, [WEB_PUBLIC_DIR / "pwa-512.png"])
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


def main() -> None:
    required_commands = ("rsvg-convert", "magick")
    missing = [command for command in required_commands if shutil.which(command) is None]
    if missing:
        raise RuntimeError(f"Missing required commands: {', '.join(missing)}")
    if not SOCIAL_FONT.exists():
        raise RuntimeError(f"Missing bundled social preview font: {SOCIAL_FONT}")
    icon_dark, icon_square, logo_dark = generate_svg_assets()
    generate_raster_assets(icon_dark, icon_square, logo_dark)
    generate_social_preview()
    print("Generated Dockrev brand assets from canonical SVG sources.")


if __name__ == "__main__":
    main()
