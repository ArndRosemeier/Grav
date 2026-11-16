"""Look for duplicated code in Grav with Arnd's Toolbox, on demand.

    python tools/dupes.py                 # crates/ of this repo
    python tools/dupes.py ../Engine/engine/src

Two finders run (both need Node, nothing to install):
  - duplicate-candidates: functions that share a name or a near name across
    files (a mechanic written twice under the same name);
  - repeated-code: function bodies that are near-identical under different
    names, and snippets pasted into three or more files.

Toolbox (github.com/ArndRosemeier/Toolbox) is used from a sibling checkout
(../Toolbox or ../toolbox) when there is one, otherwise it is cloned into
target/toolbox and pulled on every run. Reports go to target/dupes/. Both
finders list candidates for a person to judge; exit 0 means they ran.
"""

import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
URL = "https://github.com/ArndRosemeier/Toolbox.git"


def toolbox() -> Path:
    for name in ("Toolbox", "toolbox"):
        sibling = ROOT.parent / name
        if (sibling / "tools" / "repeated-code").is_dir():
            return sibling
    own = ROOT / "target" / "toolbox"
    if (own / ".git").is_dir():
        subprocess.run(["git", "-C", str(own), "pull", "-q", "--ff-only"], check=False)
    else:
        own.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(["git", "clone", "-q", "--depth", "1", URL, str(own)], check=True)
    return own


def main() -> int:
    node = shutil.which("node")
    if not node:
        print("dupes: Node is not installed (https://nodejs.org); the finders run on it.")
        return 1
    src = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / "crates"
    tb = toolbox()
    out = ROOT / "target" / "dupes"
    out.mkdir(parents=True, exist_ok=True)
    runs = [
        ("duplicate-candidates/find-duplicate-candidates.mjs", "duplicate-candidates.md"),
        ("repeated-code/find-repeated-code.mjs", "repeated-code.md"),
    ]
    for script, report in runs:
        code = subprocess.run(
            [node, str(tb / "tools" / script), "--src", str(src), "--report", str(out / report)]
        ).returncode
        if code != 0:
            return code
        print()
    print(f"dupes: full reports in {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
