"""List (page, viewport, path, property) values that differ between two Chrome dumps of the same site.

usage: python3 noise.py <chrome-a.json> <chrome-b.json> <noise.json>
"""
import json
import sys

a, b = json.load(open(sys.argv[1])), json.load(open(sys.argv[2]))
noise = []
for page, viewports in a.items():
    for vp, elements in viewports.items():
        other = b.get(page, {}).get(vp, {})
        for key in elements.keys() ^ other.keys():
            noise.append((page, vp, key, "*"))
        for key in elements.keys() & other.keys():
            noise.extend((page, vp, key, p) for p, v in elements[key].items() if other[key].get(p) != v)
json.dump(noise, open(sys.argv[3], "w"))
print(f"{len(noise)} unstable values between two Chrome runs:", sorted({n[3] for n in noise}))
