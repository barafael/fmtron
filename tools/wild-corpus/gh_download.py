"""Download the files listed in gh_index.jsonl at their pinned commit and
look up each repository's license. Re-runnable: skips files already fetched.
Writes raw/github.com/<owner>__<repo>/<path> and gh_files.jsonl.
"""
import json, os, subprocess, sys, urllib.parse, urllib.request
from concurrent.futures import ThreadPoolExecutor
from common import UA, WORK

OUT = os.path.join(WORK, "gh_files.jsonl")
LIC = os.path.join(WORK, "gh_licenses.json")
MAX_FILE = 512 * 1024

licenses = json.load(open(LIC)) if os.path.exists(LIC) else {}
done = set()
if os.path.exists(OUT):
    done = {(json.loads(l)["repo"], json.loads(l)["path"]) for l in open(OUT)}
todo = [json.loads(l) for l in open(os.path.join(WORK, "gh_index.jsonl"))]
todo = [t for t in todo if (t["repo"], t["path"]) not in done]

for repo in sorted({t["repo"] for t in todo} - licenses.keys()):
    p = subprocess.run(["gh", "api", f"repos/{repo}", "--jq", ".license.spdx_id // \"NONE\""],
                       capture_output=True, text=True)
    licenses[repo] = p.stdout.strip() if p.returncode == 0 else "UNKNOWN"
json.dump(licenses, open(LIC, "w"), indent=1)


def fetch(t):
    url = ("https://raw.githubusercontent.com/" + t["repo"] + "/" + t["ref"] + "/"
           + urllib.parse.quote(t["path"]))
    try:
        req = urllib.request.Request(url, headers={"User-Agent": UA})
        data = urllib.request.urlopen(req, timeout=60).read()
    except Exception as e:
        return None, f"{url}: {e}"
    if len(data) > MAX_FILE:
        return None, None
    dest = os.path.join(WORK, "raw", "github.com", t["repo"].replace("/", "__"), t["path"])
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    open(dest, "wb").write(data)
    return {**t, "license": licenses.get(t["repo"]), "source_url": url,
            "local": os.path.relpath(dest, WORK)}, None


with open(OUT, "a") as out, ThreadPoolExecutor(8) as pool:
    for rec, err in pool.map(fetch, todo):
        if err:
            print(err, file=sys.stderr)
        if rec:
            out.write(json.dumps(rec) + "\n")
print(f"downloaded {len(todo)} new; repos with license info: {len(licenses)}", file=sys.stderr)
