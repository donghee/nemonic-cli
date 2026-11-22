#!/usr/bin/env python3
"""Fetch only the original SDK WASM; verify its pinned SHA-256 before saving."""
from pathlib import Path
import hashlib, io, urllib.request, zipfile
URL = "https://mangoslab.blob.core.windows.net/nemonic-web-sdk/v0.9.1/nemonic-web-sdk-0.9.1.zip"
SHA = "2c3cc86266d4241bc855623a955af80f91235d4656220f8d469ed8f9055885f3"
with urllib.request.urlopen(URL, timeout=120) as r:
    archive = r.read(10 * 1024 * 1024)
with zipfile.ZipFile(io.BytesIO(archive)) as z:
    data = z.read("nemonic-web-sdk-0.9.1/dist/Nemonic.wasm")
if hashlib.sha256(data).hexdigest() != SHA:
    raise SystemExit("WASM SHA-256 mismatch: refusing to save")
root = Path(__file__).resolve().parent.parent
(root / "assets").mkdir(exist_ok=True)
(root / "assets/Nemonic.wasm").write_bytes(data)
print("Verified and saved", root / "assets/Nemonic.wasm")
