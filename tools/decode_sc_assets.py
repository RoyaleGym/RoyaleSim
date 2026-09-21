#!/usr/bin/env python3
"""Decode a modern Clash Royale build's csv_logic / tilemaps into data/raw/cr-<version>/.

WHY THIS EXISTS
    The vendored data (data/raw/retroroyale-2018) is a ~2018 client.  The live
    game changed pathfinding on 2025-03-31 and after, and the globals that
    configure the new pathfinder (NEW_PATHFINDING_CODE, PATHFINDING_*_COST, ...)
    exist only in post-2025 builds.  This tool turns a verified modern build into
    plain files the gates can read.

INPUT
    Either an already-extracted asset directory (holding csv_logic/ and
    tilemaps/) or the
    split_install_time_asset_pack.apk itself (assets/csv_logic/... inside the zip).
    Verify the APK's signature BEFORE decoding (apksigner verify --print-certs);
    this tool does not.

FORMAT
    Supercell compresses these files as LZMA "alone" streams with a truncated
    header: 5 property bytes + a 4-byte little-endian uncompressed size (the
    standard header uses 8).  Files not starting with 0x5D are copied verbatim.

OUTPUT
    data/raw/cr-<version>/{csv_logic,tilemaps,locations}/..., gitignored: these
    are Supercell's files and are not redistributed.  A MANIFEST.json records the
    sha256 of every input and output.

USAGE
    python tools/decode_sc_assets.py --version 15.535.29 --src <extracted-assets dir or asset-pack .apk>
"""

from __future__ import annotations

import argparse
import hashlib
import json
import lzma
import struct
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SUBDIRS = ("csv_logic", "tilemaps", "locations")


def decode(raw: bytes) -> bytes:
    if raw[:1] != b"\x5d":
        return raw
    size = struct.unpack("<I", raw[5:9])[0]
    header = raw[:5] + struct.pack("<Q", size)
    out = lzma.LZMADecompressor(format=lzma.FORMAT_ALONE).decompress(header + raw[9:])
    if len(out) < size:
        raise ValueError(f"short decode: {len(out)} < {size}")
    return out[:size]


def inputs(src: Path):
    """Yield (relative path, raw bytes) for every csv/toml under the wanted subdirs."""
    if src.suffix.lower() == ".apk":
        with zipfile.ZipFile(src) as z:
            for name in z.namelist():
                parts = name.split("/")
                if len(parts) >= 3 and parts[0] == "assets" and parts[1] in SUBDIRS and name.endswith((".csv", ".toml")):
                    yield "/".join(parts[1:]), z.read(name)
        return
    for sub in SUBDIRS:
        for p in sorted((src / sub).rglob("*")):
            if p.is_file() and p.suffix in (".csv", ".toml"):
                yield p.relative_to(src).as_posix(), p.read_bytes()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--version", required=True, help="client version, e.g. 15.535.29")
    ap.add_argument("--src", required=True, type=Path)
    a = ap.parse_args()
    out_root = ROOT / "data" / "raw" / f"cr-{a.version}"
    manifest = {"version": a.version, "source": str(a.src), "files": {}}
    n = 0
    for rel, raw in inputs(a.src):
        data = decode(raw)
        dst = out_root / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        dst.write_bytes(data)
        manifest["files"][rel] = {
            "in_sha256": hashlib.sha256(raw).hexdigest(),
            "out_sha256": hashlib.sha256(data).hexdigest(),
            "bytes": len(data),
        }
        n += 1
    if n == 0:
        raise SystemExit(f"no csv/toml found under {a.src}")
    (out_root / "MANIFEST.json").write_text(json.dumps(manifest, indent=1) + "\n", encoding="utf-8", newline="\n")
    print(f"decoded {n} files -> {out_root.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
