"""Collect .ron file references from GitHub code search, sliced by file size so
each query stays under the 1,000-result cap. Writes one JSON line per file to
gh_index.jsonl: repo, path, blob sha, commit ref, html_url.

Code search allows 10 requests/minute, so requests are spaced 6.5 s apart.
"""
import json, subprocess, time, sys, os, urllib.parse
from common import WORK

OUT = os.path.join(WORK, "gh_index.jsonl")
MAX_PER_REPO = 25
seen, per_repo = set(), {}
if os.path.exists(OUT):
    for line in open(OUT):
        r = json.loads(line)
        seen.add((r["repo"], r["path"]))
        per_repo[r["repo"]] = per_repo.get(r["repo"], 0) + 1

last = [0.0]


def search(q, page):
    wait = 6.5 - (time.time() - last[0])
    if wait > 0:
        time.sleep(wait)
    last[0] = time.time()
    for attempt in range(5):
        p = subprocess.run(
            ["gh", "api", "-X", "GET", "search/code", "-f", f"q={q}",
             "-f", "per_page=100", "-f", f"page={page}"],
            capture_output=True, text=True)
        if p.returncode == 0:
            return json.loads(p.stdout)
        if "rate limit" in p.stderr.lower() or "403" in p.stderr or "secondary" in p.stderr.lower():
            time.sleep(60)
            continue
        print("ERR", q, page, p.stderr[:200], file=sys.stderr)
        return None
    return None


def slice_sizes(lo, hi):
    """Yield (lo, hi) byte ranges whose result count is <= 1000."""
    stack = [(lo, hi)]
    while stack:
        a, b = stack.pop()
        r = search(f"extension:ron size:{a}..{b}", 1)
        if r is None:
            continue
        n = r["total_count"]
        print(f"slice {a}..{b}: {n}", file=sys.stderr, flush=True)
        if n > 1000 and b > a:
            m = (a + b) // 2
            stack += [(m + 1, b), (a, m)]
        else:
            yield a, b, r


def record(items, f):
    added = 0
    for it in items:
        repo, path = it["repository"]["full_name"], it["path"]
        if (repo, path) in seen or per_repo.get(repo, 0) >= MAX_PER_REPO:
            continue
        ref = urllib.parse.parse_qs(urllib.parse.urlparse(it["url"]).query).get("ref", [""])[0]
        f.write(json.dumps({"host": "github.com", "repo": repo, "path": path,
                            "sha": it["sha"], "ref": ref, "html_url": it["html_url"]}) + "\n")
        seen.add((repo, path))
        per_repo[repo] = per_repo.get(repo, 0) + 1
        added += 1
    f.flush()
    return added


def coarse(bounds):
    """Fixed slices without bisection: sample each size range evenly."""
    for a, b in zip(bounds, bounds[1:]):
        r = search(f"extension:ron size:{a}..{b - 1}", 1)
        if r is not None:
            print(f"slice {a}..{b - 1}: {r['total_count']}", file=sys.stderr, flush=True)
            yield a, b - 1, r


if sys.argv[1] == "coarse":
    pages = int(sys.argv[2])
    slices = coarse([int(x) for x in sys.argv[3:]])
else:
    lo, hi, pages = int(sys.argv[1]), int(sys.argv[2]), int(sys.argv[3])
    slices = slice_sizes(lo, hi)
with open(OUT, "a") as f:
    for a, b, first in slices:
        added = record(first["items"], f)
        for page in range(2, pages + 1):
            if (page - 1) * 100 >= min(first["total_count"], 1000):
                break
            r = search(f"extension:ron size:{a}..{b}", page)
            if not r or not r["items"]:
                break
            added += record(r["items"], f)
        print(f"  {a}..{b}: +{added} (repos {len(per_repo)}, files {len(seen)})",
              file=sys.stderr, flush=True)
