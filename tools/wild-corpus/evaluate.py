"""Merge the collected indexes, dedupe by content, run fmtron's `wild`
harness over everything, and summarize. Writes results.jsonl (one record per
unique file, with provenance + status) and prints a summary.
"""
import collections, hashlib, json, os, subprocess, sys
from common import TOOLS, WORK

WILD = os.path.join(TOOLS, "harness", "target", "release", "wild-harness")

records = []
for idx in ("gh_files.jsonl", "crates_index.jsonl", "forge_index.jsonl", "codeberg_index.jsonl"):
    p = os.path.join(WORK, idx)
    if os.path.exists(p):
        records += [json.loads(l) for l in open(p)]

by_hash, dupes = {}, 0
for r in records:
    path = os.path.join(WORK, r["local"])
    if not os.path.exists(path):
        continue
    h = hashlib.sha256(open(path, "rb").read()).hexdigest()
    r["sha256"] = h
    if h in by_hash:
        by_hash[h].setdefault("also_at", []).append(r.get("html_url") or r.get("source_url"))
        dupes += 1
        continue
    by_hash[h] = r

uniq = list(by_hash.values())
paths = "\n".join(os.path.join(WORK, r["local"]) for r in uniq)
res = subprocess.run([WILD], input=paths, capture_output=True, text=True).stdout
status = {}
for line in res.splitlines():
    path, st, detail = (line.split("\t") + ["", ""])[:3]
    status[os.path.relpath(path, WORK)] = (st, detail)

with open(os.path.join(WORK, "results.jsonl"), "w") as out:
    for r in uniq:
        r["status"], r["detail"] = status.get(r["local"], ("missing", ""))
        text = open(os.path.join(WORK, r["local"]), "rb").read().decode("utf-8", "replace").strip()
        if "\n" not in text and " " not in text and ("/" in text or text.endswith(".ron")):
            r["status"] = "symlink-stub"
        out.write(json.dumps(r) + "\n")

print(f"{len(records)} files collected, {dupes} duplicates, {len(uniq)} unique")
sources = collections.Counter(r.get("repo") or r.get("crate") for r in uniq)
print(f"{len(sources)} distinct repositories/crates")
print("by host:", dict(collections.Counter(r["host"] for r in uniq)))
tab = collections.defaultdict(collections.Counter)
for r in uniq:
    tab[r["status"]][r["host"]] += 1
for st, c in sorted(tab.items(), key=lambda kv: -sum(kv[1].values())):
    print(f"  {st:16} {sum(c.values()):6}  {dict(c)}")
