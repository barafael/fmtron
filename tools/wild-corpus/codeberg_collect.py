"""Broad Codeberg (Forgejo) collection for local evaluation.

Searches many keywords, keeps repositories written in Rust or mentioning RON,
lists each tree at the default branch's head, and downloads up to
MAX_PER_REPO .ron files. Also records LICENSE-like files found in the tree,
since Codeberg's API does not report a license.
Writes raw/codeberg.org/<owner>__<repo>/<path> and codeberg_index.jsonl.
"""
import json, os, sys, time, urllib.parse, urllib.request
from common import UA, WORK

OUT = os.path.join(WORK, "codeberg_index.jsonl")
API = "https://codeberg.org/api/v1"
MAX_PER_REPO = 40
MAX_FILE = 512 * 1024
KEYWORDS = """bevy ron game engine config rust roguelike gamedev ecs serde egui wgpu cosmic
voxel tui editor plugin simulation macroquad fyrox amethyst ggez piston sdl wasm cli tool
theme dotfiles config-files settings level map tilemap sprite shader render physics
server client daemon bot matrix fediverse gtk iced slint relm leptos yew axum tokio
parser compiler emulator synth audio music midi keyboard wayland sway hyprland niri
launcher compositor terminal shell nix neovim helix zellij gitui starship ui widget
""".split()


def get(url, as_json=True):
    time.sleep(0.35)
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    for attempt in range(4):
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                data = r.read()
                return json.loads(data) if as_json else data
        except urllib.error.HTTPError as e:
            if e.code in (429, 502, 503):
                time.sleep(20)
                continue
            raise
        except TimeoutError:
            time.sleep(5)
    raise RuntimeError("giving up: " + url)


done = set()
for idx in ("codeberg_index.jsonl", "forge_index.jsonl"):
    p = os.path.join(WORK, idx)
    if os.path.exists(p):
        done |= {json.loads(l)["repo"] for l in open(p) if json.loads(l)["host"] == "codeberg.org"}
checked = set(done)
out = open(OUT, "a")
for kw in KEYWORDS:
    for page in range(1, 11):
        try:
            res = get(f"{API}/repos/search?q={kw}&limit=50&page={page}")
        except Exception as e:
            print("search", kw, page, e, file=sys.stderr)
            break
        repos = res.get("data") or []
        if not repos:
            break
        for r in repos:
            repo = r["full_name"]
            text = f"{r.get('name', '')} {r.get('description', '')}".lower()
            if repo in checked or r.get("empty"):
                continue
            if r.get("language") != "Rust" and "ron" not in text.split():
                continue
            checked.add(repo)
            try:
                br = urllib.parse.quote(r["default_branch"], safe="")
                commit = get(f"{API}/repos/{repo}/branches/{br}")["commit"]["id"]
                tree = get(f"{API}/repos/{repo}/git/trees/{commit}?recursive=true&per_page=10000")
                blobs = [t["path"] for t in tree.get("tree", []) if t["type"] == "blob"]
                ron_files = [p for p in blobs if p.endswith(".ron")]
                if not ron_files:
                    continue
                license_files = [p for p in blobs if "/" not in p and p.upper().startswith(("LICENSE", "LICENCE", "COPYING"))]
                kept = 0
                for path in ron_files[:MAX_PER_REPO]:
                    enc = urllib.parse.quote(path)
                    data = get(f"https://codeberg.org/{repo}/raw/commit/{commit}/{enc}", as_json=False)
                    if len(data) > MAX_FILE:
                        continue
                    dest = os.path.join(WORK, "raw", "codeberg.org", repo.replace("/", "__"), path)
                    os.makedirs(os.path.dirname(dest), exist_ok=True)
                    open(dest, "wb").write(data)
                    out.write(json.dumps({
                        "host": "codeberg.org", "repo": repo, "path": path, "ref": commit,
                        "license": None, "license_files": license_files,
                        "local": os.path.relpath(dest, WORK),
                        "html_url": f"https://codeberg.org/{repo}/src/commit/{commit}/{enc}"}) + "\n")
                    kept += 1
                out.flush()
                print(f"{repo}: {kept}/{len(ron_files)} .ron, license files {license_files}",
                      file=sys.stderr, flush=True)
            except Exception as e:
                print("repo", repo, e, file=sys.stderr)
    print(f"== keyword {kw} done; repos checked {len(checked)}", file=sys.stderr, flush=True)
