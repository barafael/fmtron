"""Compare the output of fmtron builds over a corpus, to judge a style change.

usage: python3 compare_styles.py <out dir> <name>=<fmtron binary>... -- <corpus dir>...

For each build, formats every `.ron` file (ignoring `fmt.ron` files) and
prints, over the files every build formats:

- churn:   lines removed plus lines added from input to output, summed. Lower
           means closer to how people already write RON
- lines:   output lines, summed
- halved:  files (over 5 lines) whose output has under half the input's lines
- doubled: files (over 2 lines) whose output has over twice the input's lines
- differs: files whose output differs from the first build's

Writes each build's output to <out dir>/<name>/<input path>, to diff builds
against each other with `diff -r`.

Example, against the last release:

    git worktree add /tmp/fmtron-old v0.10.0
    cargo build --release --manifest-path /tmp/fmtron-old/Cargo.toml
    cargo build --release
    python3 tools/wild-corpus/compare_styles.py style-out \\
        old=/tmp/fmtron-old/target/release/fmtron new=target/release/fmtron \\
        -- test_data/wild test_data/corpus
"""
import difflib, os, subprocess, sys
from concurrent.futures import ThreadPoolExecutor

args = sys.argv[1:]
sep = args.index("--")
out_dir, builds, dirs = args[0], [b.split("=", 1) for b in args[1:sep]], args[sep + 1:]

files = sorted(
    os.path.join(r, f) for d in dirs for r, _, fs in os.walk(d) for f in fs if f.endswith(".ron")
)


def lines(s):
    return s.replace("\r\n", "\n").rstrip("\n").split("\n")


def churn(a, b):
    sm = difflib.SequenceMatcher(None, a, b, autojunk=False)
    same = sum(m.size for m in sm.get_matching_blocks())
    return (len(a) - same) + (len(b) - same)


def run(binary):
    def one(path):
        with open(path, "rb") as f:
            data = f.read()
        r = subprocess.run([binary, "--no-config"], input=data, capture_output=True)
        if r.returncode != 0:
            return path, None
        out = r.stdout.decode("utf-8", "replace")
        a, b = lines(data.decode("utf-8", "replace")), lines(out)
        return path, {"in": len(a), "out": len(b), "churn": churn(a, b), "text": out}

    with ThreadPoolExecutor(os.cpu_count()) as ex:
        return {p: r for p, r in ex.map(one, files) if r is not None}


results = {name: run(binary) for name, binary in builds}
common = set.intersection(*(set(r) for r in results.values()))
first = results[builds[0][0]]
print(f"{len(common)} files formatted by every build\n")
print(f"{'build':20} {'churn':>8} {'lines':>8} {'halved':>7} {'doubled':>8} {'differs':>8}")
for name, r in results.items():
    rs = [r[p] for p in common]
    print(
        f"{name:20} {sum(x['churn'] for x in rs):8} {sum(x['out'] for x in rs):8}"
        f" {sum(1 for x in rs if x['in'] > 5 and 2 * x['out'] < x['in']):7}"
        f" {sum(1 for x in rs if x['in'] > 2 and x['out'] > 2 * x['in']):8}"
        f" {sum(1 for p in common if r[p]['text'] != first[p]['text']):8}"
    )
print(f"\ninput lines: {sum(first[p]['in'] for p in common)}")

for name, r in results.items():
    for p in common:
        q = os.path.join(out_dir, name, os.path.relpath(p))
        os.makedirs(os.path.dirname(q), exist_ok=True)
        with open(q, "w") as f:
            f.write(r[p]["text"])
