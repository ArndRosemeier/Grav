"""Download the Actors packs (bodies, clothes, hair, animation clips) into
assets/packs, so the hero is drawn as an Actors dwarf. Without them the game
draws a block figure.

    python tools/get_packs.py            # the latest Actors release
    python tools/get_packs.py v0.35.0    # a given release

The archive is large (about 450 MB). assets/packs is git-ignored.
"""

import json
import shutil
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REPO = "ArndRosemeier/Actors"


def release(tag: str | None) -> dict:
    url = f"https://api.github.com/repos/{REPO}/releases/" + (f"tags/{tag}" if tag else "latest")
    with urllib.request.urlopen(url) as r:
        return json.load(r)


def main() -> int:
    tag = sys.argv[1] if len(sys.argv) > 1 else None
    rel = release(tag)
    asset = next((a for a in rel["assets"] if a["name"].startswith("actors-packs-")), None)
    if asset is None:
        print(f"release {rel['tag_name']} has no actors-packs archive", file=sys.stderr)
        return 1
    dest = ROOT / "assets" / "packs"
    print(f"downloading {asset['name']} ({asset['size'] / 1e6:.0f} MB) ...")
    with tempfile.TemporaryDirectory() as tmp:
        archive = Path(tmp) / asset["name"]
        with urllib.request.urlopen(asset["browser_download_url"]) as r, open(archive, "wb") as f:
            shutil.copyfileobj(r, f)
        out = Path(tmp) / "out"
        with tarfile.open(archive) as t:
            t.extractall(out, filter="data")
        src = out / "packs"
        if not (src / "human" / "manifest.json").exists():
            print("the archive has no packs/human/manifest.json", file=sys.stderr)
            return 1
        if dest.exists():
            shutil.rmtree(dest)
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(src), dest)
    (dest / "VERSION").write_text(rel["tag_name"] + "\n")
    print(f"packs {rel['tag_name']} in {dest}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
