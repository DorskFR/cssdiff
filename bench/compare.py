"""Compare the changes cssdiff reports with the changes Chrome's getComputedStyle shows.

usage: python3 compare.py <chrome-base.json> <chrome-mutant.json> <cssdiff-report.json> <noise.json> <out.json>

A change is (page, viewport, structural path, property). Properties whose
getComputedStyle value is a layout *used* value are excluded on both sides:
cssdiff compares computed values by default, which is the point of the tool.
Changes listed in <noise.json> (values that differ between two Chrome runs of
the same site) are excluded too: they are Chrome flakiness, not CSS changes.
"""
import json
import sys

LAYOUT = {
    "width", "height", "min-width", "min-height", "top", "right", "bottom", "left",
    "transform-origin", "perspective-origin", "grid-template-rows", "grid-template-columns",
}

base, mutant = json.load(open(sys.argv[1])), json.load(open(sys.argv[2]))
report = json.load(open(sys.argv[3]))
noise = {tuple(n) for n in json.load(open(sys.argv[4]))}

chrome = set()
for page, viewports in base.items():
    for vp, elements in viewports.items():
        other = mutant.get(page, {}).get(vp, {})
        for key in elements.keys() ^ other.keys():
            chrome.add((page, vp, key, "*"))
        for key in elements.keys() & other.keys():
            for prop, value in elements[key].items():
                if prop not in LAYOUT and other[key].get(prop) != value:
                    chrome.add((page, vp, key, prop))

ours = set()
for page in report["pages"]:
    for c in page["changes"]:
        if c["property"] not in LAYOUT:
            ours.update((page["page"], vp, c["path"], c["property"]) for vp in c["viewports"])
    for u in page["unmatched"]:
        ours.update((page["page"], vp, u["path"], "*") for vp in u["viewports"])

chrome -= noise
ours -= noise
both = chrome & ours
result = {
    "chrome": len(chrome),
    "cssdiff": len(ours),
    "both": len(both),
    "verdict": (
        "agree-unchanged" if not chrome and not ours
        else "agree-changed" if chrome and ours
        else "MISSED" if chrome
        else "FALSE-ALARM"
    ),
    "chrome_only": sorted(chrome - ours)[:20],
    "cssdiff_only": sorted(ours - chrome)[:20],
}
json.dump(result, open(sys.argv[5], "w"), indent=1)
print(f"{result['verdict']:16} chrome={len(chrome):6} cssdiff={len(ours):6} both={len(both):6}")
