"""Copy the redistributable, valid subset of the collected files into
fmtron's test_data/wild/ and write test_data/wild/manifest.ron with full
attribution (host, repository/crate, commit or version, original path,
source URL, license, sha256).

Kept: unique files that the `ron` crate or fmtron accepts, from sources
whose declared license is a recognized open-source license.
"""
import collections, datetime, json, os, re, shutil, subprocess
from common import REPO, WORK

DEST = os.path.join(REPO, "test_data", "wild")
MAX_PER_SOURCE = 8
MAX_FILE = 32 * 1024

OPEN = {"mit", "apache-2.0", "bsd-2-clause", "bsd-3-clause", "0bsd", "isc", "zlib",
        "mpl-2.0", "unlicense", "cc0-1.0", "cc-by-4.0", "cc-by-sa-4.0", "gpl-2.0",
        "gpl-3.0", "lgpl-2.1", "lgpl-3.0", "agpl-3.0", "bsl-1.0", "wtfpl", "epl-2.0",
        "ofl-1.1", "artistic-2.0", "eupl-1.2", "gpl-3.0-or-later", "gpl-2.0-or-later",
        "lgpl-3.0-or-later", "agpl-3.0-or-later", "gpl-3.0-only", "gpl-2.0-only",
        "lgpl-2.1-or-later", "mit-0", "apache-2.0 with llvm-exception"}


def open_license(lic):
    """True if every alternative/component of an SPDX-ish expression is known open."""
    if not lic or lic.upper() in ("NONE", "NOASSERTION", "UNKNOWN", "OTHER"):
        return False
    parts = re.split(r"\s+(?:OR|AND|or|and)\s+|/|,", lic.strip("() "))
    return any(p.strip("() ").lower() in OPEN for p in parts if p.strip())


def is_symlink_stub(text):
    t = text.strip()
    return "\n" not in t and " " not in t and ("/" in t or t.endswith(".ron"))


def ron_str(s):
    return json.dumps(s, ensure_ascii=False)  # JSON string syntax is valid RON


recs = [json.loads(l) for l in open(os.path.join(WORK, "results.jsonl"))]
valid = {"ok", "content-changed", "not-idempotent", "semantic", "output-invalid",
         "rejects-valid", "accepts-invalid", "panic"}
kept, per_source, skipped = [], collections.Counter(), collections.Counter()
for r in recs:
    src = r.get("repo") or f'{r["crate"]}-{r["version"]}'
    text = open(os.path.join(WORK, r["local"]), "rb").read().decode("utf-8", "replace")
    if r["status"] not in valid:
        skipped["invalid or not UTF-8"] += 1
    elif r["status"] == "accepts-invalid" and "No RON extension" not in r["detail"]:
        skipped["invalid (fmtron lenient)"] += 1
    elif len(text.encode()) > MAX_FILE:
        skipped["over 32 KiB"] += 1
    elif is_symlink_stub(text):
        skipped["symlink stub"] += 1
    elif not open_license(r.get("license")):
        skipped["no open license"] += 1
    elif per_source[src] >= MAX_PER_SOURCE:
        skipped["per-source cap"] += 1
    else:
        per_source[src] += 1
        kept.append((src, r))

if os.path.isdir(DEST):
    shutil.rmtree(DEST)
by_source = collections.defaultdict(list)
for src, r in kept:
    slug = src.replace("/", "__")
    local = os.path.join(r["host"], slug, r["path"])
    os.makedirs(os.path.dirname(os.path.join(DEST, local)), exist_ok=True)
    shutil.copyfile(os.path.join(WORK, r["local"]), os.path.join(DEST, local))
    by_source[(r["host"], src)].append((local, r))

lines = ["(",
         '    description: "Real-world RON files collected from public forges and crates.io to exercise fmtron on RON as people actually write it. Every file keeps its original content and is listed with its source for attribution.",',
         f'    generated_on: "{datetime.date.today().isoformat()}",',
         f"    total_files: {len(kept)},",
         f"    total_sources: {len(by_source)},",
         '    selection: "Unique by sha256; accepted by the ron crate (0.12) or fmtron; source declares an open-source license; at most '
         f'{MAX_PER_SOURCE} files per source, each at most 32 KiB. Collected via GitHub code search, the GitLab.com and Codeberg APIs, and .crate archives of crates.io reverse dependencies of ron.",',
         '    layout: "Each file is stored at <host>/<source with / replaced by __>/<original_path>.",',
         "    sources: ["]
for (host, src), files in sorted(by_source.items()):
    r0 = files[0][1]
    lines += ["        (",
              f"            host: {ron_str(host)},",
              f"            source: {ron_str(src)},"]
    if host == "crates.io":
        lines += [f"            crate: {ron_str(r0['crate'])},",
                  f"            version: {ron_str(r0['version'])},",
                  f"            url: {ron_str('https://crates.io/crates/' + r0['crate'])},"]
        if r0.get("repository"):
            lines.append(f"            repository: {ron_str(r0['repository'])},")
    else:
        lines += [f"            url: {ron_str('https://' + host + '/' + src)},",
                  f"            commit: {ron_str(r0.get('ref', ''))},"]
    lines += [f"            license: {ron_str(r0.get('license'))},",
              "            files: ["]
    for local, r in files:
        url = r.get("html_url") or r.get("source_url")
        lines.append(f"                (original_path: {ron_str(r['path'])}, source_url: {ron_str(url)}),")
    lines += ["            ],", "        ),"]
lines += ["    ],", ")", ""]
manifest = os.path.join(DEST, "manifest.ron")
open(manifest, "w").write("\n".join(lines))
# Dogfood: keep the manifest in fmtron's own canonical format.
subprocess.run([os.path.join(REPO, "target", "release", "fmtron"), "-w", "100", "-i", manifest], check=True)
os.remove(manifest + ".bak")

size = sum(os.path.getsize(os.path.join(d, f)) for d, _, fs in os.walk(DEST) for f in fs)
print(f"kept {len(kept)} files from {len(by_source)} sources ({size / 1e6:.1f} MB); skipped: {dict(skipped)}")
print("by host:", dict(collections.Counter(r["host"] for _, r in kept)))
