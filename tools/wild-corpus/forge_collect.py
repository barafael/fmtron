"""Collect .ron files from GitLab.com and Codeberg (Forgejo) via their public
APIs: search projects by keyword, list each repository tree, download up to
MAX_PER_REPO .ron files. Writes files under raw/<host>/<owner>__<repo>/ and
one JSON line per file to forge_index.jsonl.
"""
import json, os, sys, time, urllib.parse, urllib.request
from common import UA, WORK

OUT = os.path.join(WORK, "forge_index.jsonl")
MAX_PER_REPO = 25
MAX_FILE = 512 * 1024
KEYWORDS = ["bevy", "ron", "game", "engine", "config", "rust", "veloren", "amethyst",
            "roguelike", "gamedev", "ecs", "serde", "egui", "wgpu", "cosmic", "voxel",
            "tui", "editor", "plugin", "simulation"]


def get(url, as_json=True):
    time.sleep(0.4)
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    for attempt in range(4):
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                data = r.read()
                return json.loads(data) if as_json else data
        except urllib.error.HTTPError as e:
            if e.code == 429:
                time.sleep(30)
                continue
            raise
    raise RuntimeError("rate limited: " + url)


done_repos = set()
if os.path.exists(OUT):
    done_repos = {(json.loads(l)["host"], json.loads(l)["repo"]) for l in open(OUT)}
out = open(OUT, "a")


def save(host, repo, path, data, meta):
    safe = repo.replace("/", "__")
    dest = os.path.join(WORK, "raw", host, safe, path)
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    open(dest, "wb").write(data)
    out.write(json.dumps({"host": host, "repo": repo, "path": path,
                          "local": os.path.relpath(dest, WORK), **meta}) + "\n")
    out.flush()


def gitlab():
    G = "https://gitlab.com/api/v4"
    seen = set()
    for kw in KEYWORDS:
        for page in range(1, 4):
            try:
                projects = get(f"{G}/projects?search={kw}&with_programming_language=Rust"
                               f"&per_page=100&page={page}&order_by=star_count")
            except Exception as e:
                print("gitlab search", kw, e, file=sys.stderr)
                break
            if not projects:
                break
            for p in projects:
                repo = p["path_with_namespace"]
                if repo in seen or ("gitlab.com", repo) in done_repos or not p.get("default_branch"):
                    continue
                seen.add(repo)
                try:
                    ron_files = []
                    for tpage in range(1, 30):
                        tree = get(f"{G}/projects/{p['id']}/repository/tree?recursive=true"
                                   f"&per_page=100&page={tpage}")
                        ron_files += [t["path"] for t in tree
                                      if t["type"] == "blob" and t["path"].endswith(".ron")]
                        if len(tree) < 100:
                            break
                    if not ron_files:
                        continue
                    info = get(f"{G}/projects/{p['id']}?license=true")
                    lic = (info.get("license") or {}).get("key")
                    commit = get(f"{G}/projects/{p['id']}/repository/commits/"
                                 f"{urllib.parse.quote(p['default_branch'], safe='')}")["id"]
                    for path in ron_files[:MAX_PER_REPO]:
                        enc = urllib.parse.quote(path, safe="")
                        data = get(f"{G}/projects/{p['id']}/repository/files/{enc}/raw?ref={commit}",
                                   as_json=False)
                        if len(data) > MAX_FILE:
                            continue
                        save("gitlab.com", repo, path, data, {
                            "ref": commit, "license": lic,
                            "html_url": f"https://gitlab.com/{repo}/-/blob/{commit}/{path}"})
                    print(f"gitlab {repo}: {min(len(ron_files), MAX_PER_REPO)}/{len(ron_files)} .ron ({lic})",
                          file=sys.stderr, flush=True)
                except Exception as e:
                    print("gitlab repo", repo, e, file=sys.stderr)


def codeberg():
    C = "https://codeberg.org/api/v1"
    seen = set()
    for kw in KEYWORDS:
        for page in range(1, 6):
            try:
                res = get(f"{C}/repos/search?q={kw}&limit=50&page={page}")
            except Exception as e:
                print("codeberg search", kw, e, file=sys.stderr)
                break
            if not res.get("data"):
                break
            for r in res["data"]:
                repo = r["full_name"]
                if repo in seen or ("codeberg.org", repo) in done_repos or r.get("empty"):
                    continue
                seen.add(repo)
                try:
                    br = r["default_branch"]
                    b = get(f"{C}/repos/{repo}/branches/{urllib.parse.quote(br, safe='')}")
                    commit = b["commit"]["id"]
                    tree = get(f"{C}/repos/{repo}/git/trees/{commit}?recursive=true&per_page=10000")
                    ron_files = [t["path"] for t in tree.get("tree", [])
                                 if t["type"] == "blob" and t["path"].endswith(".ron")]
                    if not ron_files:
                        continue
                    lic = ",".join(r.get("licenses") or []) or None
                    for path in ron_files[:MAX_PER_REPO]:
                        enc = urllib.parse.quote(path)
                        data = get(f"https://codeberg.org/{repo}/raw/commit/{commit}/{enc}", as_json=False)
                        if len(data) > MAX_FILE:
                            continue
                        save("codeberg.org", repo, path, data, {
                            "ref": commit, "license": lic,
                            "html_url": f"https://codeberg.org/{repo}/src/commit/{commit}/{enc}"})
                    print(f"codeberg {repo}: {min(len(ron_files), MAX_PER_REPO)}/{len(ron_files)} .ron ({lic})",
                          file=sys.stderr, flush=True)
                except Exception as e:
                    print("codeberg repo", repo, e, file=sys.stderr)


{"gitlab": gitlab, "codeberg": codeberg}[sys.argv[1]]()
