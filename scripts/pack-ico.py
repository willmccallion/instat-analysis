"""Packs PNG renders of the app icon into a Windows .ico file.

usage: pack-ico.py RENDER_DIR OUTPUT.ico
RENDER_DIR holds icon_<pixels>.png for 16, 32, 64, 128 and 256.
"""

import struct
import sys
from pathlib import Path

SIZES = [16, 32, 64, 128, 256]
PNG_MAGIC = b"\x89PNG\r\n\x1a\n"
HEADER_SIZE = 6
ENTRY_SIZE = 16


def main() -> None:
    render_dir, output = Path(sys.argv[1]), Path(sys.argv[2])
    images = []
    for pixels in SIZES:
        data = (render_dir / f"icon_{pixels}.png").read_bytes()
        if not data.startswith(PNG_MAGIC):
            sys.exit(f"icon_{pixels}.png is not a PNG")
        images.append((pixels, data))
    offset = HEADER_SIZE + ENTRY_SIZE * len(images)
    entries = []
    for pixels, data in images:
        # Width and height are stored in one byte each; 0 means 256.
        side = pixels % 256
        entries.append(struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(data), offset))
        offset += len(data)
    header = struct.pack("<HHH", 0, 1, len(images))
    output.write_bytes(header + b"".join(entries) + b"".join(data for _, data in images))


if __name__ == "__main__":
    main()
