"""Packs PNG renders of the app icon into a macOS .icns file.

usage: pack-icns.py RENDER_DIR OUTPUT.icns
RENDER_DIR holds icon_<pixels>.png for 16, 32, 64, 128, 256, 512 and 1024.
"""

import struct
import sys
from pathlib import Path

# (icns type, pixel size): modern macOS reads PNG data for every one of these types.
ENTRIES = [
    (b"icp4", 16), (b"icp5", 32), (b"icp6", 64),
    (b"ic07", 128), (b"ic08", 256), (b"ic09", 512), (b"ic10", 1024),
    (b"ic11", 32), (b"ic12", 64), (b"ic13", 256), (b"ic14", 512),
]
PNG_MAGIC = b"\x89PNG\r\n\x1a\n"


def main() -> None:
    render_dir, output = Path(sys.argv[1]), Path(sys.argv[2])
    chunks = []
    for kind, pixels in ENTRIES:
        data = (render_dir / f"icon_{pixels}.png").read_bytes()
        if not data.startswith(PNG_MAGIC):
            sys.exit(f"icon_{pixels}.png is not a PNG")
        chunks.append(kind + struct.pack(">I", len(data) + 8) + data)
    body = b"".join(chunks)
    output.write_bytes(b"icns" + struct.pack(">I", len(body) + 8) + body)


if __name__ == "__main__":
    main()
