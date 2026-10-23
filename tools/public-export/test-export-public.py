#!/usr/bin/env python3
"""Regression probe for the Spanish-stopword gate of export-public.py.

Reproduces the abort the exporter suffered on any tree whose prose tripped the
stopword detector: inside the detector loop `hits` (the dict that accumulates
pii / retail_text / secret:* / internal_name) was rebound to a *list* of Spanish
tokens, so the first `hits.get(...)` after the loop raised AttributeError and the
export never reached its verdict.

The gate measures human prose only, with the extraction and word list owned by
`scripts/ci/language-check.py` (CR-abe5ff3d): counting raw file tokens flagged
identifiers such as `y_rotation`, `i32_le` or `z1`, which are homographs of
Spanish function words.

Three hermetic scenarios, each on a throwaway git repository (the exporter only
reads `git HEAD` of the repo that owns the script):

  spanish      the exporter must finish and report FAIL, with the check
               `spanish_stopwords_absent` carrying `file:line:phrase`, while every
               other check keeps the values of the dict-based scan (proof that `hits`
               survived the language loop).
  clean        the exporter must finish and report PASS with 0 flagged files
               (delta 0) and an exported file set identical to HEAD.
  identifiers  English Rust whose only Spanish-looking tokens are identifiers and
               short English words must PASS: the gate may not read code as prose.

The publication-origin gate (B7) is probed on the same fixtures: a clean tree alone is
not a licence to publish, so the exporter must also see a declared release commit.

  release-none      clean fixture, no tag, no `--release-commit`: the exporter must
                    abort with a non-zero exit, name the missing declaration and leave
                    the destination untouched.
  release-stale     `--release-commit` naming a commit that does not resolve to HEAD:
                    same abort, on the override path.
  release-tag       a milestone tag `M0-2026-10-21` on the fixture HEAD (fixture-only —
                    the real M18 tag is never created here) must let the export run and
                    report `declared_by == tag:M0-2026-10-21`.

Usage:
  python3 tools/public-export/test-export-public.py [--keep]

Exit 0 = every scenario behaves as specified. No network, no cargo, no writes
outside the temporary fixtures.
"""
import argparse, json, os, shutil, subprocess, sys, tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
EXPORT = HERE / "export-public.py"
EXCLUDE = HERE / "exclude.txt"
LANG_GATE = "scripts/ci/language-check.py"

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
# The gate reports the extracted prose line: comments lose their `//` marker.
SPANISH_WHERE = "%s:1:%s" % (SPANISH_FILE, SPANISH_LINES[0].strip().lstrip("/").lstrip())

# English Rust whose only Spanish-looking tokens live in identifiers and short English
# words ("y", "le", "no", "se", "al"): the token-level gate flagged these as Spanish
# The prose gate must ignore code entirely and flag none of it.
IDENTIFIER_FILES = {
    "crates/parser-bevy-bridge/src/interior_spec.rs":
        "//! - Daggerfall: Y up, left-handed; Bevy: Y up, right-handed.\n"
        "//! Y is the vertical axis, so no rotation is applied here.\n"
        "pub fn point(data: &[u8], at: usize) -> i32 {\n"
        "    let (x1, y1, z1) = (0, 1, 2);\n"
        "    let _ = (x1, y1, z1);\n"
        "    i32_le(data, at + 4)\n"
        "}\n",
    "crates/parser/src/woods.rs":
        "pub fn clamp_y(y: i32) -> i32 {\n"
        "    let y = y.clamp(0, 100);\n"
        "    let value = i32_le(&[y as u8], 0);\n"
        "    // Clamp y so the axis stays inside the map.\n"
        "    value.clamp(0, 100)\n"
        "}\n",
}
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


def make_repo(root, spanish, identifiers=False):
    files = dict(BASE_FILES)
    if spanish:
        files[SPANISH_FILE] = "\n".join(SPANISH_LINES) + "\n"
    if identifiers:
        files.update(IDENTIFIER_FILES)
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
    # The exporter reuses the CI gate's prose extraction; the fixture keeps the same
    # relative path so the import resolves exactly as it does in this repository.
    lang_src = HERE.parents[1] / "scripts" / "ci" / "language-check.py"
    lang_dst = root / "scripts" / "ci" / "language-check.py"
    lang_dst.parent.mkdir(parents=True, exist_ok=True)
    lang_dst.write_bytes(lang_src.read_bytes())
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


def run_scenario(name, spanish, keep, identifiers=False, release="override", tag=None):
    """Export a fixture; returns (report, returncode, stderr, exported set, HEAD set).

    `release` selects the publication-origin mode (B7): `override` passes the fixture
    HEAD through `--release-commit`, `tag` marks it with a milestone tag instead,
    `none` declares nothing and `stale` names a commit that is not HEAD. The last two
    must abort before writing, so they return `report = None`.
    """
    root = Path(tempfile.mkdtemp(prefix="export-probe-%s-" % name))
    try:
        repo = root / "repo"
        repo.mkdir()
        make_repo(repo, spanish, identifiers)
        head = git(repo, "rev-parse", "HEAD").strip()
        if tag:
            git(repo, "tag", tag)
        out = root / "export"
        cmd = [sys.executable, str(repo / "tools" / "public-export" / "export-public.py"),
               "--out", str(out)]
        if release == "override":
            cmd += ["--release-commit", head]
        elif release == "stale":
            cmd += ["--release-commit", "0" * 40]
        res = subprocess.run(cmd, cwd=repo, capture_output=True, text=True, timeout=900)
        report_path = out.parent / (out.name + ".report.json")
        if not report_path.exists():
            if release in ("none", "stale"):
                expect(res.returncode != 0, "[%s] expected a non-zero exit, got 0" % name)
                expect(not out.exists(), "[%s] aborted export created %s" % (name, out))
                return (None, res.returncode, res.stderr, [], head_paths(repo))
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
    # scanned counts prose-bearing files: README.md, src/lib.rs, src/notes.rs and the
    # CI extractor copied at scripts/ci/language-check.py.
    expect(chk["files"] == 1 and chk["scanned"] == 4,
           "[spanish] files=%r scanned=%r, expected 1 / 4" % (chk.get("files"), chk.get("scanned")))
    expect(chk["thresholds"] == {"min_es_tokens": 2, "max_es_ratio": 0.15, "min_accents": 2},
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
    # The extractor ships an accent character class: pattern data, not prose
    # (docs/i18n/STATUS.md). No other file may carry Spanish-specific accents.
    surface = report["surface"]["spanish_surface"]
    unexpected = [f for f in surface["sample"] if f != LANG_GATE]
    expect(not unexpected and surface["files"] <= 1,
           "[clean] spanish_surface delta != 0: %r" % (surface,))
    chk = report["checks"]["spanish_stopwords_absent"]
    expect(chk["pass"] is True and chk["scanned"] == 3,
           "[clean] spanish_stopwords_absent %r, expected pass with 3 scanned" % (chk,))
    expect(report["files_dropped"] == 0,
           "[clean] dropped %d files, expected 0" % report["files_dropped"])
    expect(exported == head, "[clean] file-set drift\nexported=%r\nHEAD=%r" % (exported, head))


def check_identifiers(report, code, stderr):
    expect(code == 0, "[identifiers] expected exit 0 (PASS verdict), got %d: %s"
           % (code, (stderr or "").strip()[-400:]))
    expect(report["verdict"] == "PASS",
           "[identifiers] verdict %r, expected PASS" % report["verdict"])
    lang = report["surface"]["language"]
    expect(lang["flagged"] == [] and lang["flagged_count"] == 0,
           "[identifiers] identifiers flagged as Spanish prose: %r" % (lang["flagged"],))
    chk = report["checks"]["spanish_stopwords_absent"]
    # README.md, src/lib.rs, the two identifier files and scripts/ci/language-check.py.
    expect(chk["pass"] is True and chk["scanned"] == 5,
           "[identifiers] spanish_stopwords_absent %r, expected pass with 5 scanned" % (chk,))
    others = [k for k, v in report["checks"].items() if v["pass"] is not True]
    expect(not others, "[identifiers] unrelated checks failed: %r" % (others,))


def check_release_abort(name, code, stderr, needle):
    expect(code != 0, "[%s] export must abort, got exit %d" % (name, code))
    expect(needle in (stderr or ""),
           "[%s] stderr must carry %r, got %r" % (name, needle, (stderr or "").strip()[-300:]))


def check_release_override(report):
    rel = report["release"]
    expect(rel["declared_by"].startswith("override:") and rel["declared_by"].endswith(rel["head"]),
           "[release-override] declared_by %r, head %r" % (rel["declared_by"], rel["head"]))


def check_release_tag(report):
    expect(report["verdict"] == "PASS", "[release-tag] verdict %r" % report["verdict"])
    expect(report["release"]["declared_by"] == "tag:M0-2026-10-21",
           "[release-tag] declared_by %r" % (report["release"],))


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
    check_release_override(report)
    print("PASS clean: PASS verdict, delta 0, %d files match HEAD, release by override" % len(head))

    _, code, stderr, _, _ = run_scenario("release-none", False, a.keep, release="none")
    check_release_abort("release-none", code, stderr, "not a declared release commit")
    print("PASS release-none: clean tree without a declared release aborts")

    _, code, stderr, _, _ = run_scenario("release-stale", False, a.keep, release="stale")
    check_release_abort("release-stale", code, stderr, "does not resolve to HEAD")
    print("PASS release-stale: --release-commit that is not HEAD aborts")

    report, code, stderr, exported, head = run_scenario(
        "release-tag", False, a.keep, release="tag", tag="M0-2026-10-21")
    check_clean(report, code, stderr, exported, head)
    check_release_tag(report)
    print("PASS release-tag: milestone tag M0-2026-10-21 authorises the export")

    report, code, stderr = run_scenario("identifiers", False, a.keep, identifiers=True)[:3]
    check_identifiers(report, code, stderr)
    print("PASS identifiers: PASS verdict, 0 of %d prose files flagged"
          % report["checks"]["spanish_stopwords_absent"]["scanned"])
    print("OK: export-public.py stopword + publication-origin gates behave as specified")


if __name__ == "__main__":
    main()
