"""Compare two style dumps value by value and report agreement per property.

usage: python3 agreement.py <chrome.json> <cssdiff.json>
"""
import json
import sys
from collections import Counter

chrome = json.load(open(sys.argv[1]))
ours = json.load(open(sys.argv[2]))

same, total, examples = Counter(), Counter(), {}
keys_only = Counter()
for page, viewports in chrome.items():
    for vp, elements in viewports.items():
        other = ours.get(page, {}).get(vp, {})
        keys_only["chrome"] += len(elements.keys() - other.keys())
        keys_only["cssdiff"] += len(other.keys() - elements.keys())
        for key in elements.keys() & other.keys():
            for prop, value in elements[key].items():
                total[prop] += 1
                if other[key].get(prop) == value:
                    same[prop] += 1
                else:
                    examples.setdefault(prop, Counter())[(value, other[key].get(prop))] += 1

values = sum(total.values())
print(f"elements only in chrome: {keys_only['chrome']}, only in cssdiff: {keys_only['cssdiff']}")
print(f"values compared: {values}, identical: {sum(same.values())} ({100 * sum(same.values()) / values:.2f}%)")
perfect = sum(1 for p in total if same[p] == total[p])
print(f"properties identical on every element: {perfect}/{len(total)}")
print("\nproperties that differ (agreement, top chrome → cssdiff value pairs):")
for prop in sorted(total, key=lambda p: same[p] / total[p]):
    if same[prop] == total[prop]:
        continue
    top = ", ".join(f"{a!r}→{b!r}×{n}" for (a, b), n in examples[prop].most_common(2))
    print(f"  {prop:32} {100 * same[prop] / total[prop]:6.2f}%  {top[:150]}")
