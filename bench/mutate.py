"""Create mutated copies of a built site, each with one realistic CSS edit.

Each copy is a symlink tree of the site (cheap) with the edited stylesheet
replaced by a real file. Writes <out>/mut-NN/ and <out>/mutations.json.

usage: python3 mutate.py <site-dir> <out-dir> <count> <seed>
"""
import json
import random
import re
import shutil
import subprocess
import sys
from pathlib import Path

site, out, count, seed = Path(sys.argv[1]), Path(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
rng = random.Random(seed)
sheets = sorted(p for p in site.rglob("*.css") if "pagefind" not in p.parts)

DECL = re.compile(r"(?<=[{;])\s*([a-z-]+)\s*:\s*([^;{}]+?)(?=[;}])")
KEYWORDS = {
    "display": ["block", "flex", "grid", "inline", "none", "inline-block"],
    "position": ["static", "relative", "absolute"],
    "text-align": ["left", "center", "right"],
    "font-weight": ["400", "700", "300"],
    "align-items": ["center", "flex-start", "stretch"],
    "justify-content": ["center", "space-between", "flex-start"],
    "flex-direction": ["row", "column"],
    "overflow": ["hidden", "auto", "visible"],
    "white-space": ["nowrap", "normal", "pre"],
}
COLOR = re.compile(r"#[0-9a-fA-F]{3,8}\b|(?:rgb|rgba|hsl|hsla)\([^)]*\)")
LENGTH = re.compile(r"(-?\d*\.?\d+)(px|rem|em)\b")
MEDIA = re.compile(r"(@media[^{]*?(?:min|max)-width\s*:\s*)(\d*\.?\d+)(px|rem|em)")


def mutate(css):
    kind = rng.choice(["color", "length", "keyword", "delete", "media"])
    if kind == "media":
        hits = list(MEDIA.finditer(css))
        if not hits:
            return None
        m = rng.choice(hits)
        bumped = float(m[2]) + (100 if m[3] == "px" else 6)
        new = f"{m[1]}{bumped:g}{m[3]}"
        return kind, css[: m.start()] + new + css[m.end():], m[0], new
    decls = list(DECL.finditer(css))
    rng.shuffle(decls)
    for m in decls:
        prop, value = m[1], m[2]
        if kind == "delete":
            return kind, css[: m.start()] + css[m.end():], m[0].strip(), ""
        if kind == "color" and (c := COLOR.search(value)):
            new = value[: c.start()] + "#c0ffee" + value[c.end():]
        elif kind == "length" and (l := LENGTH.search(value)):
            new = value[: l.start()] + f"{float(l[1]) + 1:g}{l[2]}" + value[l.end():]
        elif kind == "keyword" and prop in KEYWORDS and value.strip() in KEYWORDS[prop]:
            new = rng.choice([k for k in KEYWORDS[prop] if k != value.strip()])
        else:
            continue
        start = m.start(2)
        return kind, css[:start] + new + css[m.end(2):], f"{prop}:{value}", f"{prop}:{new}"
    return None


out.mkdir(parents=True, exist_ok=True)
mutations = []
while len(mutations) < count:
    sheet = rng.choice(sheets)
    css = sheet.read_text()
    result = mutate(css)
    if not result:
        continue
    kind, new_css, before, after = result
    name = f"mut-{len(mutations):02d}"
    dest = out / name
    if dest.exists():
        shutil.rmtree(dest)
    subprocess.run(["cp", "-as", str(site.resolve()), str(dest)], check=True)
    target = dest / sheet.relative_to(site)
    target.unlink()
    target.write_text(new_css)
    mutations.append({"name": name, "kind": kind, "sheet": str(sheet.relative_to(site)), "before": before[:120], "after": after[:120]})

(out / "mutations.json").write_text(json.dumps(mutations, indent=2))
for m in mutations:
    print(f"{m['name']} {m['kind']:8} {m['sheet']}: {m['before']!r} -> {m['after']!r}")
