#!/usr/bin/env python3
"""Fetch GeekNews RSS entries and render them to a 1-bit PNG for nemonic-cli.

Output is 576px wide (MIP-001 print width), capped under 2000px tall per the
renderer limit. Depends on ImageMagick (`magick`) with Pango and a bold
Korean font path supplied via --font.
"""
import argparse
import html
import re
import subprocess
import sys
import urllib.request
from datetime import datetime
from pathlib import Path
from xml.etree import ElementTree as ET

RSS_URL = "https://feeds.feedburner.com/geeknews-feed"
ATOM = "{http://www.w3.org/2005/Atom}"
WIDTH_PX = 576
TEXT_WIDTH_PX = 540
BORDER = "18x12"
DEFAULT_FONT = "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc"


def strip_html(s: str) -> str:
    s = re.sub(r"<br\s*/?>", "\n", s, flags=re.I)
    s = re.sub(r"</(p|li|ul|div)>", "\n", s, flags=re.I)
    s = re.sub(r"<li[^>]*>", "- ", s, flags=re.I)
    s = re.sub(r"<[^>]+>", "", s)
    s = html.unescape(s)
    s = re.sub(r"\n{3,}", "\n\n", s)
    return s.strip()


def fetch_entries(start: int, count: int):
    data = urllib.request.urlopen(RSS_URL, timeout=10).read()
    root = ET.fromstring(data)
    return root.findall(f"{ATOM}entry")[start : start + count]


def build_text(start: int, count: int, max_body: int) -> str:
    entries = fetch_entries(start, count)
    out = [
        "GeekNews 최근 소식",
        datetime.now().strftime("%Y-%m-%d %H:%M"),
        "─" * 28,
    ]
    for i, e in enumerate(entries, start + 1):
        title = e.find(f"{ATOM}title").text.strip()
        body = strip_html(e.find(f"{ATOM}content").text or "").replace("…", "...")
        if len(body) > max_body:
            body = body[:max_body].rsplit(" ", 1)[0] + "..."
        link = e.find(f"{ATOM}id").text.strip()
        out += ["", f"[{i}] {title}", "", body, "", link, "─" * 28]
    return "\n".join(out)


def pango_escape(s: str) -> str:
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def render_png(text: str, out_path: Path, font: str, pointsize: int, threshold: str | None) -> None:
    cmd = [
        "magick",
        "-background", "white",
        "-fill", "black",
        "-stroke", "black",
        "-strokewidth", "1.0",
        "-font", font,
        "-pointsize", str(pointsize),
        "-size", f"{TEXT_WIDTH_PX}x",
        f"pango:{pango_escape(text)}",
        "-bordercolor", "white",
        "-border", BORDER,
        "-colorspace", "Gray",
    ]
    if threshold:
        cmd += ["-threshold", threshold]
    cmd.append(str(out_path))
    subprocess.run(cmd, check=True)


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--start", type=int, default=0, help="0-based entry offset")
    p.add_argument("--count", type=int, default=2, help="number of entries")
    p.add_argument("--max-body", type=int, default=260, help="max chars per entry body")
    p.add_argument("--font", default=DEFAULT_FONT, help="TTF/OTF/TTC path (Korean bold recommended)")
    p.add_argument("--pointsize", type=int, default=20)
    p.add_argument("--threshold", default=None,
                   help="ImageMagick -threshold value (e.g. 60%%). Omit to keep grayscale so nemonic-cli --dither works.")
    p.add_argument("-o", "--output", default="geeknews.png", help="PNG output path")
    p.add_argument("--print-text", action="store_true", help="also print the composed text to stdout")
    args = p.parse_args()

    text = build_text(args.start, args.count, args.max_body)
    if args.print_text:
        print(text)
    render_png(text, Path(args.output), args.font, args.pointsize, args.threshold)
    return 0


if __name__ == "__main__":
    sys.exit(main())
