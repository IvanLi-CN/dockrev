#!/usr/bin/env python3
"""Verify Dockrev's generated install-icon contract without external Python packages."""

from __future__ import annotations

import hashlib
import json
import math
import os
import posixpath
import re
import struct
import sys
import zlib
from html.parser import HTMLParser
from pathlib import Path


WEB_DIR = Path(__file__).resolve().parents[1]
PUBLIC_DIR = WEB_DIR / "public"
DIST_DIR = WEB_DIR / "dist"
BACKGROUND = (0x01, 0x0E, 0x2D)
RAW_BASE_PATH = os.environ.get("DOCKREV_WEB_BASE", "/")


def normalize_base_path(base_path: str) -> str:
    raw = base_path.strip()
    if not raw or raw == "/":
        return "/"
    with_leading_slash = raw if raw.startswith("/") else f"/{raw}"
    return with_leading_slash if with_leading_slash.endswith("/") else f"{with_leading_slash}/"


BASE_PATH = normalize_base_path(RAW_BASE_PATH)
REGULAR_HASHES = {
    "pwa-192.png": "3d6999d34c2d4dfc64c91e1515729daaabae10a4dddca950f8a3017b8fcd0d8e",
    "pwa-512.png": "a55419e5b9851669099f9e664566ed90061fa9c5b222bac892d3cfbbcff6caf8",
}
INSTALL_ICON_ASSETS = (
    "favicon.svg",
    "favicon.png",
    "favicon.ico",
    "pwa-192.png",
    "pwa-512.png",
    "pwa-maskable-192.png",
    "pwa-maskable-512.png",
)
HASH_LENGTH = 12
MANIFEST_ICON_ASSETS = (
    "pwa-192.png",
    "pwa-512.png",
    "pwa-maskable-192.png",
    "pwa-maskable-512.png",
)
HTML_ICON_ASSETS = (
    "favicon.svg",
    "favicon.png",
    "favicon.ico",
)


class LinkTagParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.links: list[dict[str, str]] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag.lower() != "link":
            return
        self.links.append({name.lower(): value or "" for name, value in attrs})


def parse_link_tags(html: str) -> list[dict[str, str]]:
    parser = LinkTagParser()
    parser.feed(html)
    parser.close()
    return parser.links


def rel_tokens(link: dict[str, str]) -> set[str]:
    return {token.lower() for token in link.get("rel", "").split()}


class ModuleTagParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.module_sources: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag.lower() != "script":
            return
        attributes = {name.lower(): value or "" for name, value in attrs}
        if attributes.get("type", "").lower() == "module" and attributes.get("src"):
            self.module_sources.append(attributes["src"])


def parse_module_sources(html: str) -> list[str]:
    parser = ModuleTagParser()
    parser.feed(html)
    parser.close()
    return parser.module_sources


def dist_path_for_url(url: str) -> str:
    pathname = url.split("?", 1)[0].split("#", 1)[0]
    if pathname.startswith(BASE_PATH):
        return pathname[len(BASE_PATH) :].lstrip("/")
    return pathname.lstrip("/")


def standalone_not_found_modules() -> dict[str, str]:
    page = (DIST_DIR / "404.html").read_text()
    pending = [dist_path_for_url(source) for source in parse_module_sources(page)]
    modules: dict[str, str] = {}
    while pending:
        module_path = pending.pop()
        if module_path in modules:
            continue
        module_file = DIST_DIR / module_path
        assert module_file.is_file(), f"404 document references missing module {module_path}"
        source = module_file.read_text()
        modules[module_path] = source
        for specifier in re.findall(r"(?:from\s*|import\()(['\"])([^'\"]+)\1", source):
            import_path = specifier[1]
            if import_path.startswith(("http:", "https:", "data:")):
                continue
            if import_path.startswith("."):
                imported_module = posixpath.normpath(posixpath.join(posixpath.dirname(module_path), import_path))
            elif import_path.startswith("/"):
                imported_module = dist_path_for_url(import_path)
            else:
                continue
            if imported_module.endswith(".js"):
                pending.append(imported_module)
    return modules


def assert_standalone_not_found_contract() -> None:
    not_found_html = (DIST_DIR / "404.html").read_text()
    assert "manifest" not in not_found_html, "404 document must not declare PWA metadata"
    assert "__DOCKREV_CONFIG__" not in not_found_html, "404 document must not inject runtime config"
    modules = standalone_not_found_modules()
    assert modules, "404 document must load its isolated entry module"
    forbidden = ("__DOCKREV_CONFIG__", "selfUpgradeUrl", "supervisor-misroute", "registerSW")
    for module_path, source in modules.items():
        assert not any(token in source for token in forbidden), (
            f"404 module {module_path} imports main-app runtime behavior"
        )


def read_png(path: Path) -> tuple[int, int, list[tuple[int, int, int, int]]]:
    raw = path.read_bytes()
    if raw[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"{path}: not a PNG")

    cursor = 8
    width = height = color_type = None
    compressed = bytearray()
    while cursor < len(raw):
        length = struct.unpack(">I", raw[cursor : cursor + 4])[0]
        chunk_type = raw[cursor + 4 : cursor + 8]
        chunk = raw[cursor + 8 : cursor + 8 + length]
        cursor += length + 12
        if chunk_type == b"IHDR":
            width, height, bit_depth, color_type, compression, filtering, interlace = struct.unpack(
                ">IIBBBBB", chunk
            )
            if bit_depth != 8 or color_type not in (2, 6) or compression or filtering or interlace:
                raise ValueError(f"{path}: unsupported PNG encoding")
        elif chunk_type == b"IDAT":
            compressed.extend(chunk)
        elif chunk_type == b"IEND":
            break

    if width is None or height is None or color_type is None:
        raise ValueError(f"{path}: missing IHDR")
    channels = 4 if color_type == 6 else 3
    scanlines = zlib.decompress(compressed)
    stride = width * channels
    if len(scanlines) != (stride + 1) * height:
        raise ValueError(f"{path}: invalid scanline length")

    rows: list[bytes] = []
    previous = bytearray(stride)
    offset = 0
    for _ in range(height):
        filter_type = scanlines[offset]
        source = scanlines[offset + 1 : offset + 1 + stride]
        offset += stride + 1
        row = bytearray(stride)
        for index, value in enumerate(source):
            left = row[index - channels] if index >= channels else 0
            up = previous[index]
            up_left = previous[index - channels] if index >= channels else 0
            if filter_type == 0:
                result = value
            elif filter_type == 1:
                result = value + left
            elif filter_type == 2:
                result = value + up
            elif filter_type == 3:
                result = value + ((left + up) // 2)
            elif filter_type == 4:
                prediction = left + up - up_left
                distances = (abs(prediction - left), abs(prediction - up), abs(prediction - up_left))
                result = value + (left if distances[0] <= distances[1] and distances[0] <= distances[2] else up if distances[1] <= distances[2] else up_left)
            else:
                raise ValueError(f"{path}: unknown PNG filter {filter_type}")
            row[index] = result & 0xFF
        rows.append(bytes(row))
        previous = row

    pixels = []
    for row in rows:
        for index in range(0, stride, channels):
            red, green, blue = row[index : index + 3]
            alpha = row[index + 3] if channels == 4 else 255
            pixels.append((red, green, blue, alpha))
    return width, height, pixels


def assert_maskable(path: Path, expected_size: int) -> None:
    width, height, pixels = read_png(path)
    assert (width, height) == (expected_size, expected_size), f"{path}: unexpected dimensions"
    assert all(alpha == 255 for _, _, _, alpha in pixels), f"{path}: maskable surface is not opaque"
    foreground = [
        (index % width, index // width)
        for index, (red, green, blue, _) in enumerate(pixels)
        if max(abs(red - BACKGROUND[0]), abs(green - BACKGROUND[1]), abs(blue - BACKGROUND[2])) >= 16
    ]
    assert foreground, f"{path}: foreground is missing"
    xs, ys = zip(*foreground)
    ratio = max(max(xs) - min(xs) + 1, max(ys) - min(ys) + 1) / width
    assert 0.58 <= ratio <= 0.62, f"{path}: foreground ratio {ratio:.3f} is outside 58%-62%"
    center = width / 2
    radius = width * 0.4
    assert all(math.hypot(x + 0.5 - center, y + 0.5 - center) <= radius for x, y in foreground), (
        f"{path}: important foreground escapes the 40% safe circle"
    )


def content_hashed_file_name(asset: str) -> str:
    source = PUBLIC_DIR / asset
    path = Path(asset)
    digest = hashlib.sha256(source.read_bytes()).hexdigest()[:HASH_LENGTH]
    return f"{path.stem}-{digest}{path.suffix}"


def assert_hashed_assets_exist() -> None:
    for asset in INSTALL_ICON_ASSETS:
        expected_name = content_hashed_file_name(asset)
        built = DIST_DIR / expected_name
        assert built.is_file(), f"build output omits content-hashed {asset}"
        assert built.read_bytes() == (PUBLIC_DIR / asset).read_bytes(), (
            f"built {expected_name} does not contain the source bytes for {asset}"
        )


def assert_build_contract() -> None:
    assert DIST_DIR.is_dir(), "build output is missing; run the PWA build before this checker"
    assert_standalone_not_found_contract()
    route_contract = json.loads((DIST_DIR / ".dockrev-route-contract.json").read_text())
    assert route_contract["version"] == 1, "route contract version is unsupported"
    assert route_contract["basePath"] == BASE_PATH, "route contract base path does not match the build"
    assert route_contract["dynamicSegmentPattern"] == "[A-Za-z0-9][A-Za-z0-9_-]{0,127}", (
        "route contract dynamic segment grammar drifted"
    )
    assert "/" in route_contract["staticPagePaths"], "route contract omits the root page"
    assert route_contract["dynamicPageTemplates"], "route contract omits dynamic page templates"
    assert route_contract["reservedPrefixes"] == ["/api", "/supervisor", "/assets"], (
        "route contract reserved prefixes drifted"
    )
    assert not list(DIST_DIR.glob("apple-touch-icon*.png")), (
        "product build must not publish Apple touch icon fallbacks"
    )
    assert_hashed_assets_exist()
    manifest = json.loads((DIST_DIR / "manifest.webmanifest").read_text())
    assert manifest["id"] == BASE_PATH, "manifest id must remain the stable base identity"
    assert manifest["scope"] == BASE_PATH, "manifest scope must remain the stable base identity"
    assert manifest["start_url"] == BASE_PATH, "manifest start_url must remain the stable base identity"
    expected_icons = [f"{BASE_PATH}{content_hashed_file_name(asset)}" for asset in MANIFEST_ICON_ASSETS]
    actual_icons = [icon["src"] for icon in manifest["icons"]]
    assert actual_icons == expected_icons, "built manifest icon URLs do not match content-hashed assets"
    assert all("?" not in src for src in actual_icons), "manifest icon URLs must not use query versions"

    built_html = (DIST_DIR / "index.html").read_text()
    links = parse_link_tags(built_html)
    manifest_links = [link for link in links if "manifest" in rel_tokens(link)]
    assert len(manifest_links) == 1, "built HTML must contain one manifest link"
    assert manifest_links[0].get("href") == f"{BASE_PATH}manifest.webmanifest", (
        "built HTML manifest link must use the stable manifest URL"
    )
    assert not any("apple-touch-icon" in rel_tokens(link) for link in links), (
        "product HTML must not declare an Apple touch icon"
    )
    assert "?v=" not in built_html, "built install metadata must not use query-versioned URLs"
    favicon_links = [link for link in links if "icon" in rel_tokens(link)]
    actual_favicon_urls = [link.get("href") for link in favicon_links]
    expected_favicon_urls = [f"{BASE_PATH}{content_hashed_file_name(asset)}" for asset in HTML_ICON_ASSETS]
    assert actual_favicon_urls == expected_favicon_urls, "built HTML favicon URLs are stale"
    worker = (DIST_DIR / "sw.js").read_text()
    assert "?v=" not in worker, "service worker must not pin query-versioned install assets"
    assert "registration.scope" in worker, "service worker must derive its app base from registration scope"
    assert "index.html" in worker, "service worker must keep an app-shell navigation fallback"
    assert "manifest.webmanifest" not in worker, "Workbox precache must not pin the manifest"
    assert "apple-touch-icon" not in worker, "service worker must not pin Apple touch icons"
    for asset in INSTALL_ICON_ASSETS:
        hashed_name = content_hashed_file_name(asset)
        assert hashed_name not in worker, f"service worker must not pin {hashed_name}"
        assert f'"{asset}"' not in worker, f"Workbox precache pins legacy {asset}"


def main() -> None:
    assert not list(PUBLIC_DIR.glob("apple-touch-icon*.png")), (
        "product source must not retain Apple touch icon fallbacks"
    )
    for name, expected_hash in REGULAR_HASHES.items():
        actual_hash = hashlib.sha256((PUBLIC_DIR / name).read_bytes()).hexdigest()
        assert actual_hash == expected_hash, f"{name}: regular baseline bytes changed"

    assert_maskable(PUBLIC_DIR / "pwa-maskable-192.png", 192)
    assert_maskable(PUBLIC_DIR / "pwa-maskable-512.png", 512)
    assert (
        hashlib.sha256((PUBLIC_DIR / "pwa-512.png").read_bytes()).digest()
        != hashlib.sha256((PUBLIC_DIR / "pwa-maskable-512.png").read_bytes()).digest()
    ), "regular and maskable icons share bytes"

    config = (WEB_DIR / "vite.config.ts").read_text()
    assert "purpose: 'any'" in config and "purpose: 'maskable'" in config, "manifest purposes are incomplete"
    assert "purpose: 'any maskable'" not in config, "manifest reuses a combined purpose"
    assert "contentHashedFileName" in config, "install metadata URLs are not content-hashed"
    assert "includeManifestIcons: false" in config, "PWA plugin may re-add fixed public icon names"
    assert "globPatterns" in config, "Workbox install asset patterns are not explicit"
    assert "apple-touch-icon" not in config, "product Vite config must not generate Apple touch icons"
    service_worker = (WEB_DIR / "src" / "sw.ts").read_text()
    assert "ignoreURLParametersMatching" not in service_worker, "service worker must not rely on query-version matching"
    assert "self.registration.scope" in service_worker, "service worker must derive its base path from registration scope"
    assert "createHandlerBoundToURL('/index.html')" not in service_worker, "service worker shell URL must not be root-hardcoded"
    index = (WEB_DIR / "index.html").read_text()
    source_links = parse_link_tags(index)
    assert not any("manifest" in rel_tokens(link) for link in source_links), (
        "manifest must be injected by the PWA plugin exactly once"
    )
    assert not any("apple-touch-icon" in rel_tokens(link) for link in source_links), (
        "product source HTML must not declare an Apple touch icon"
    )
    docs_config = (WEB_DIR.parent / "docs-site" / "rspress.config.ts").read_text()
    assert "appleTouchIconVersion" in docs_config, "docs-site Apple touch URL is not content-versioned"
    assert_build_contract()


if __name__ == "__main__":
    try:
        main()
    except (AssertionError, ValueError) as error:
        print(f"PWA asset contract failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
    print("PWA asset contract passed.")
