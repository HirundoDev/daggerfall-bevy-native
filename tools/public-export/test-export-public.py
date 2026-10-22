#!/usr/bin/env python3
"""Regression probe for the Spanish-stopword gate of export-public.py.

Reproduces the abort the exporter suffered on any tree whose prose tripped the
stopword detector: inside the detector loop `hits` (the dict that accumulates
pii / retail_text / secret:* / internal_name) was rebound to a *list* of Spanish
tokens, so the first `hits.get(...)` after the loop raised AttributeError and the
export never reached its verdict.

Two hermetic scenarios, each on a throwaway git repository (the exporter only
reads `git HEAD` of the repo that owns the script):

  spanish  the exporter must finish and report FAIL, with the check
           `spanish_stopwords_absent` carrying `file:line:phrase`, while every
           other check keeps the values of the dict-based scan (proof that `hits`
           survived the stopword loop).
  clean    the exporter must finish and report PASS with 0 flagged files
           (delta 0) and an exported file set identical to HEAD.

Usage:
  python3 tools/public-export/test-export-public.py [--keep]

Exit 0 = both scenarios behave as specified. No network, no cargo, no writes
outside the temporary fixtures.
"""
import argparse, json, os, shutil, subprocess, sys, tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
EXPORT = HERE / "export-public.py"
EXCLUDE = HERE / "exclude.txt"

# Minimal source tree the exporter demands: README.md and LICENSE are read
# unconditionally; NOTICE is optional. None of them carries a declared transform,
# a secret, an email, an owner marker or an internal name.
BASE_FILES = {
    "README.md": "# daggerfall-bevy-native\n\nA small fixture used by the public-export probe.\n",
    "LICENSE": "MIT License\n\nCopyright (c) 2026 HirundoDev\n",
    "NOTICE": "Fixture notice for the export probe.\n\nCopyright (c) 2026 HirundoDev\n",
    "src/lib.rs": "// Returns the player's remaining health.\npub fn health() -> u32 { 1 }\n",
}
SPANISH_LINES = [
    "// Esta funcion devuelve el valor de la vida del jugador",
    "// y no se puede usar para el resto de la partida",
    "pub fn vida() -> u32 { 1 }",
]
SPANISH_FILE = "src/notes.rs"
SPANISH_WHERE = "%s:1:%s" % (SPANISH_FILE, SPANISH_LINES[0].strip())
# Composed piecewise like the exporter's own COMMIT_ID: this file is part of the
# exported surface, and the `emails_other_absent` check scans tools/public-export/
# before its SCAN_SKIP cut, so a literal address here would fail the gate.
PROBE_EMAIL = "probe" + "@" + "example.invalid"


def git(root, *args):
    cmd = ["git", "-c", "user.name=probe", "-c", "user.email=" + PROBE_EMAIL,
           "-c", "commit.gpgsign=false", *args]
    res = subprocess.run(cmd, cwd=root, capture_output=True, text=True)
    if res.returncode != 0:
        raise SystemExit("git %s failed in %s: %s%s" % (" ".join(args), root, res.stdout, res.stderr))
    return res.stdout


def make_repo(root, spanish):
    files = dict(BASE_FILES)
    if spanish:
        files[SPANISH_FILE] = "\n".join(SPANISH_LINES) + "\n"
    for rel, text in files.items():
        dst = root / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        dst.write_text(text, encoding="utf-8")
    # The exporter resolves its source repo from its own location and reads
    # exclude.txt from there: the fixture gets a byte-identical copy of both.
    tool_dir = root / "tools" / "public-export"
    tool_dir.mkdir(parents=True, exist_ok=True)
    for src in (EXPORT, EXCLUDE):
        (tool_dir / src.name).write_bytes(src.read_bytes())
    git(root, "init", "-q", "-b", "main")
    git(root, "add", "--force", "--", ".")
    git(root, "commit", "-q", "--no-verify", "-m", "fixture")


def head_paths(root):
    raw = git(root, "ls-tree", "-r", "--name-only", "-z", "HEAD")
    return sorted(p for p in raw.split("\0") if p)


def exported_paths(out):
    return sorted(str(p.relative_to(out)).replace(os.sep, "/")
                  for p in out.rglob("*")
                  if p.is_file() and ".git" not in p.relative_to(out).parts)


def run_scenario(name, spanish, keep):
    """Export a fixture; returns (report, returncode, stderr, exported set, HEAD set)."""
    root = Path(tempfile.mkdtemp(prefix="export-probe-%s-" % name))
    try:
        repo = root / "repo"
        repo.mkdir()
        make_repo(repo, spanish)
        out = root / "export"
        res = subprocess.run([sys.executable,
                              str(repo / "tools" / "public-export" / "export-public.py"),
                              "--out", str(out)],
                             cwd=repo, capture_output=True, text=True, timeout=900)
        report_path = out.parent / (out.name + ".report.json")
        if not report_path.exists():
            tail = "\n".join((res.stderr or res.stdout).strip().splitlines()[-8:])
            if "AttributeError" in (res.stderr or ""):
                tail += ("\nREGRESSION: `hits` was rebound to a list — the stopword loop must use "
                         "its own variable (stopword_hits).")
            raise AssertionError("[%s] exporter produced no report (exit %d):\n%s"
                                 % (name, res.returncode, tail))
        report = json.loads(report_path.read_text(encoding="utf-8"))
        return (report, res.returncode, res.stderr, exported_paths(out), head_paths(repo))
    finally:
        if keep:
            print("kept fixture: %s" % root)
        else:
            shutil.rmtree(root, ignore_errors=True)


def expect(cond, msg):
    if not cond:
        raise AssertionError(msg)


def check_spanish(report, code):
    expect(code == 1, "[spanish] expected exit 1 (FAIL verdict), got %d" % code)
    expect(report["verdict"] == "FAIL", "[spanish] verdict %r, expected FAIL" % report["verdict"])
    chk = report["checks"]["spanish_stopwords_absent"]
    expect(chk["pass"] is False, "[spanish] spanish_stopwords_absent must not pass")
    expect(chk["files"] == 1 and chk["scanned"] == 3,
           "[spanish] files=%r scanned=%r, expected 1 / 3" % (chk.get("files"), chk.get("scanned")))
    expect(chk["thresholds"] == {"min_es_tokens": 3, "max_es_ratio": 0.15},
           "[spanish] thresholds drifted: %r" % (chk["thresholds"],))
    expect(chk["sample"] == [SPANISH_WHERE],
           "[spanish] sample %r, expected [%r]" % (chk["sample"], SPANISH_WHERE))
    flagged = report["surface"]["language"]["flagged"]
    expect([f["where"] for f in flagged] == [SPANISH_WHERE],
           "[spanish] flagged where %r" % ([f["where"] for f in flagged],))
    # The dict-based scan must keep working after the stopword loop: the original
    # AttributeError fired exactly here, on the first hits.get().
    for k in ("pii_paths", "retail_text", "public_name_absent_internal", "secrets"):
        expect(report["checks"][k]["pass"] is True, "[spanish] check %s must pass" % k)
    others = [k for k, v in report["checks"].items()
              if k != "spanish_stopwords_absent" and v["pass"] is not True]
    expect(not others, "[spanish] unrelated checks failed: %r" % (others,))


def check_clean(report, code, stderr, exported, head):
    expect(code == 0, "[clean] expected exit 0 (PASS verdict), got %d: %s"
           % (code, (stderr or "").strip()[-400:]))
    expect(report["verdict"] == "PASS", "[clean] verdict %r, expected PASS" % report["verdict"])
    lang = report["surface"]["language"]
    expect(lang["flagged_count"] == 0 and lang["flagged"] == [],
           "[clean] delta != 0: %r" % (lang["flagged"],))
    expect(report["surface"]["spanish_surface"]["files"] == 0,
           "[clean] spanish_surface delta != 0: %r" % (report["surface"]["spanish_surface"],))
    chk = report["checks"]["spanish_stopwords_absent"]
    expect(chk["pass"] is True and chk["scanned"] == 2,
           "[clean] spanish_stopwords_absent %r, expected pass with 2 scanned" % (chk,))
    expect(report["files_dropped"] == 0,
           "[clean] dropped %d files, expected 0" % report["files_dropped"])
    expect(exported == head, "[clean] file-set drift\nexported=%r\nHEAD=%r" % (exported, head))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--keep", action="store_true", help="keep the fixtures and exported trees")
    a = ap.parse_args()
    if not EXPORT.exists() or not EXCLUDE.exists():
        raise SystemExit("missing %s or %s" % (EXPORT, EXCLUDE))

    check_spanish(*run_scenario("spanish", True, a.keep)[:2])
    print("PASS spanish: FAIL verdict, %s" % SPANISH_WHERE)

    report, code, stderr, exported, head = run_scenario("clean", False, a.keep)
    check_clean(report, code, stderr, exported, head)
    print("PASS clean: PASS verdict, delta 0, %d files match HEAD" % len(head))
    print("OK: export-public.py stopword gate behaves as specified")


if __name__ == "__main__":
    main()
