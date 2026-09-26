"""Collect .ron files from published crates that depend on `ron`.

For each reverse dependency (latest version that depends on ron), download
the .crate archive from static.crates.io and keep the .ron files in it.
Writes files under raw/crates.io/<crate>-<version>/ and one JSON line per
file to crates_index.jsonl (crate, version, license, repository, path).
crates.io asks for <= 1 API request/second and a descriptive User-Agent.
"""
import io, json, os, sys, tarfile, time, urllib.request
from common import UA, WORK

OUT = os.path.join(WORK, "crates_index.jsonl")
RAW = os.path.join(WORK, "raw", "crates.io")
MAX_FILE = 512 * 1024
MAX_PER_CRATE = 25


def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read()


def api(path):
    time.sleep(1.0)
    return json.loads(get("https://crates.io/api/v1" + path))


done = set()
if os.path.exists(OUT):
    done = {json.loads(l)["crate"] for l in open(OUT)}

pages = int(sys.argv[1]) if len(sys.argv) > 1 else 50
out = open(OUT, "a")
for page in range(1, pages + 1):
    data = api(f"/crates/ron/reverse_dependencies?per_page=100&page={page}")
    versions = {v["id"]: v for v in data["versions"]}
    if not data["dependencies"]:
        break
    for dep in data["dependencies"]:
        v = versions.get(dep["version_id"])
        if not v or v["crate"] in done:
            continue
        name, num = v["crate"], v["num"]
        done.add(name)
        try:
            blob = get(f"https://static.crates.io/crates/{name}/{name}-{num}.crate")
        except Exception as e:
            print("download failed", name, num, e, file=sys.stderr)
            continue
        kept = 0
        try:
            with tarfile.open(fileobj=io.BytesIO(blob), mode="r:gz") as tar:
                for m in tar.getmembers():
                    if not (m.isfile() and m.name.endswith(".ron") and m.size <= MAX_FILE):
                        continue
                    if kept >= MAX_PER_CRATE:
                        break
                    rel = m.name.split("/", 1)[1]
                    dest = os.path.join(RAW, f"{name}-{num}", rel)
                    os.makedirs(os.path.dirname(dest), exist_ok=True)
                    with open(dest, "wb") as f:
                        f.write(tar.extractfile(m).read())
                    out.write(json.dumps({
                        "host": "crates.io", "crate": name, "version": num,
                        "license": v.get("license"), "repository": v.get("repository"),
                        "path": rel, "local": os.path.relpath(dest, WORK),
                        "source_url": f"https://static.crates.io/crates/{name}/{name}-{num}.crate",
                    }) + "\n")
                    kept += 1
        except Exception as e:
            print("extract failed", name, e, file=sys.stderr)
        out.flush()
        print(f"page {page} {name}-{num}: {kept} .ron", file=sys.stderr, flush=True)
