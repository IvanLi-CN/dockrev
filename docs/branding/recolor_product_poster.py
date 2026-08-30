"""Render the light product-poster theme from the approved dark master."""

from __future__ import annotations

import argparse
import subprocess
from pathlib import Path


LIGHT_SURFACE = (0.984, 0.992, 1.0)
LIGHT_INK = (0.063, 0.157, 0.259)
LIGHT_TEXT = (1.0, 1.0, 1.0)
ACCENT_HUES = (
    (0.48, 0.70, "#0c79cf"),  # cyan and blue
    (0.22, 0.48, "#138347"),  # success green
    (0.08, 0.22, "#a45f00"),  # warning amber
    (0.94, 0.08, "#be2e35"),  # error red
    (0.70, 0.94, "#6743b7"),  # worker purple
)


def clamp(value: float, lower: float = 0.0, upper: float = 1.0) -> float:
    return max(lower, min(upper, value))


def smoothstep(value: float, start: float, end: float) -> float:
    progress = clamp((value - start) / (end - start))
    return progress * progress * (3.0 - 2.0 * progress)


def rgb_to_hsv(red: float, green: float, blue: float) -> tuple[float, float, float]:
    maximum = max(red, green, blue)
    minimum = min(red, green, blue)
    delta = maximum - minimum
    saturation = delta / maximum if maximum > 0.0 else 0.0
    if delta == 0.0:
        return 0.0, saturation, maximum
    if maximum == red:
        hue = ((green - blue) / delta) % 6.0
    elif maximum == green:
        hue = (blue - red) / delta + 2.0
    else:
        hue = (red - green) / delta + 4.0
    return (hue / 6.0) % 1.0, saturation, maximum


def hex_to_rgb(value: str) -> tuple[float, float, float]:
    return tuple(int(value[index : index + 2], 16) / 255.0 for index in (1, 3, 5))


def hue_gate(hue: float, lower: float, upper: float) -> float:
    if lower <= upper:
        return smoothstep(hue, lower - 0.02, lower + 0.02) * (
            1.0 - smoothstep(hue, upper - 0.02, upper + 0.02)
        )
    return max(
        smoothstep(hue, lower - 0.02, lower + 0.02),
        1.0 - smoothstep(hue, upper - 0.02, upper + 0.02),
    )


def dilate(mask: bytearray, width: int, height: int, radius: int) -> bytearray:
    """Apply a square binary dilation with two linear-time sliding passes."""
    horizontal = bytearray(width * height)
    window_size = radius * 2 + 1
    for row in range(height):
        row_start = row * width
        window_count = 0
        for column in range(width + radius):
            if column < width:
                window_count += mask[row_start + column]
            remove_column = column - window_size
            if remove_column >= 0:
                window_count -= mask[row_start + remove_column]
            output_column = column - radius
            if 0 <= output_column < width:
                horizontal[row_start + output_column] = 1 if window_count else 0

    vertical = bytearray(width * height)
    for column in range(width):
        window_count = 0
        for row in range(height + radius):
            if row < height:
                window_count += horizontal[row * width + column]
            remove_row = row - window_size
            if remove_row >= 0:
                window_count -= horizontal[remove_row * width + column]
            output_row = row - radius
            if 0 <= output_row < height:
                vertical[output_row * width + column] = 1 if window_count else 0
    return vertical


def image_dimensions(source_path: Path) -> tuple[int, int]:
    result = subprocess.run(
        ["magick", "identify", "-format", "%w %h", str(source_path)],
        check=True,
        capture_output=True,
        text=True,
    )
    width, height = (int(value) for value in result.stdout.split())
    return width, height


def read_rgb(source_path: Path, width: int, height: int) -> bytes:
    result = subprocess.run(
        ["magick", str(source_path), "-depth", "8", "RGB:-"],
        check=True,
        capture_output=True,
    )
    expected_length = width * height * 3
    if len(result.stdout) != expected_length:
        raise RuntimeError(
            f"Expected {expected_length} RGB bytes, received {len(result.stdout)}"
        )
    return result.stdout


def write_rgb(output_path: Path, rgb: bytearray, width: int, height: int) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            "magick",
            "-depth",
            "8",
            "-size",
            f"{width}x{height}",
            "RGB:-",
            "-strip",
            "-define",
            "png:exclude-chunk=tIME",
            str(output_path),
        ],
        input=bytes(rgb),
        check=True,
    )


def recolor(source_path: Path, output_path: Path) -> None:
    width, height = image_dimensions(source_path)
    source = read_rgb(source_path, width, height)
    output = bytearray(len(source))
    accent_mask = bytearray(width * height)
    accent_targets = tuple(
        (lower, upper, hex_to_rgb(color)) for lower, upper, color in ACCENT_HUES
    )

    for pixel_index in range(width * height):
        source_index = pixel_index * 3
        red, green, blue = (source[source_index + channel] / 255.0 for channel in range(3))
        luma = red * 0.2126 + green * 0.7152 + blue * 0.0722
        hue, saturation, value = rgb_to_hsv(red, green, blue)

        ink_weight = clamp((luma - 0.08) / 0.83) ** 0.95
        base = tuple(
            LIGHT_SURFACE[channel] * (1.0 - ink_weight) + LIGHT_INK[channel] * ink_weight
            for channel in range(3)
        )
        accent_weight = smoothstep(saturation, 0.35, 0.75) * smoothstep(value, 0.20, 0.45)
        coverage = 0.0
        accent = [0.0, 0.0, 0.0]
        shade = 0.52 + 0.48 * smoothstep(value, 0.18, 0.96)
        for lower, upper, target in accent_targets:
            coverage_part = hue_gate(hue, lower, upper)
            coverage += coverage_part
            for channel in range(3):
                tinted = LIGHT_SURFACE[channel] * (1.0 - shade) + target[channel] * shade
                accent[channel] += tinted * coverage_part
        if coverage > 1e-6:
            accent = [channel / coverage for channel in accent]
        else:
            accent = list(base)

        for channel in range(3):
            value_out = base[channel] * (1.0 - accent_weight) + accent[channel] * accent_weight
            output[source_index + channel] = round(clamp(value_out) * 255.0)
        accent_mask[pixel_index] = 1 if accent_weight > 0.72 else 0

    embedded_mask = dilate(accent_mask, width, height, radius=6)
    for pixel_index in range(width * height):
        source_index = pixel_index * 3
        red, green, blue = (source[source_index + channel] / 255.0 for channel in range(3))
        luma = red * 0.2126 + green * 0.7152 + blue * 0.0722
        _, saturation, _ = rgb_to_hsv(red, green, blue)
        bright_neutral = smoothstep(luma, 0.62, 0.90) * (1.0 - smoothstep(saturation, 0.08, 0.30))
        embedded_weight = bright_neutral * embedded_mask[pixel_index]
        if embedded_weight <= 0.0:
            continue
        for channel in range(3):
            current = output[source_index + channel] / 255.0
            output[source_index + channel] = round(
                clamp(current * (1.0 - embedded_weight) + LIGHT_TEXT[channel] * embedded_weight) * 255.0
            )

    write_rgb(output_path, output, width, height)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    recolor(args.source, args.output)


if __name__ == "__main__":
    main()
