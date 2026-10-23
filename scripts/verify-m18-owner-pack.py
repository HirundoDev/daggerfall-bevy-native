#!/usr/bin/env python3
"""verify-m18-owner-pack.py - mechanical cross-check of the M18 owner pack.

Recomputes, from the evidence on disk: every CS row is present exactly once,
every capture referenced by the pack exists in the run directory, its sha256
matches the PNG bytes, and the PASS/PARCIAL/NO counters in the pack equal the
rows. It does NOT re-judge the visual verdicts (that is the owner's step).
"""
import hashlib
import json
import os
import re
import sys

GATE = "docs/sintergic/l5-mission/m18-owner-gate"
RUN = GATE + "/evidence/run-window-audio-b8cd155"
pack = open(GATE + "/M18-OWNER-SIGNATURE-PACK.md").read()
side = open(GATE + "/m18-side-by-side.md").read()
machine = json.load(open(GATE + "/m18-ve-machine.json"))
steps = [json.loads(l) for l in open(RUN + "/steps.jsonl")]
by_label = {s["label"]: s for s in steps}

fails = []
cs_rows = re.findall(r"^\| (CS-\d\d) \| [a-i/]+ \| (\w+) \|", pack, re.M)
if len(cs_rows) != 21:
    fails.append("pack rows=%d (expected 21)" % len(cs_rows))
if len({c for c, _ in cs_rows}) != 21:
    fails.append("duplicate CS rows in the pack")
for cs, est in cs_rows:
    if est not in ("PASS", "PARCIAL", "NO"):
        fails.append("%s invalid state %s" % (cs, est))
    if cs not in side:
        fails.append("%s missing from the side-by-side file" % cs)

caps = machine["captures"]
if len(caps) != len(steps):
    fails.append("captures ve-machine=%d vs steps=%d" % (len(caps), len(steps)))
for cap in caps:
    path = cap["png"]
    if not os.path.isfile(path):
        fails.append("missing PNG %s" % path)
        continue
    got = hashlib.sha256(open(path, "rb").read()).hexdigest()
    if got != cap["sha256"]:
        fails.append("%s sha256 mismatch" % path)
    # the pack cites the 16-hex prefix; the full digest lives in ve-machine.json
    if cap["sha256"][:16] not in pack:
        fails.append("%s sha not cited in the pack" % cap["label"])
    if cap["label"] not in by_label:
        fails.append("%s not present in steps.jsonl" % cap["label"])

counts = {"PASS": 0, "PARCIAL": 0, "NO": 0}
for _, est in cs_rows:
    counts[est] = counts.get(est, 0) + 1
m = re.search(r"\*\*(\d+) PASS · (\d+) PARCIAL · (\d+) NO\*\*", pack)
if not m:
    fails.append("the pack counter is not found")
else:
    stated = {"PASS": int(m.group(1)), "PARCIAL": int(m.group(2)), "NO": int(m.group(3))}
    if stated != counts:
        fails.append("pack counter %s != rows %s" % (stated, counts))

print(json.dumps({"rows": len(cs_rows), "counts": counts, "captures": len(caps), "fails": fails}, indent=2))
sys.exit(1 if fails else 0)
