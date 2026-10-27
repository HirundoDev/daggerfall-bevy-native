#!/usr/bin/env python3
"""Sanitized export of Daggerfall-like Engine into a single local commit.

Usage:
  python3 tools/public-export/export-public.py                # dry-run to a tempdir
  python3 tools/public-export/export-public.py --out /tmp/x   # concrete dir (must not exist or --force)

Hard rules (CR-f9a78b9a, CR-5a5c759b): never push, never remote, never rewrite history.
Publication origin (B7): the exporter only copies a **declared release commit** — a clean
worktree is necessary but not sufficient. HEAD must carry a milestone tag `M<n>-YYYY-MM-DD`,
be the tip of a `release/*` branch, or be named by `--release-commit <sha>`. Without one of
the three it aborts before touching the destination.
Copies the content of **git HEAD** (not the working tree: other lanes leave edits in flight),
applies exclude.txt, applies the declared public surface (public name + links to non-exported
paths), runs `git init` + a single commit with a deterministic date, and executes the checks.
Exit 0 = all PASS.
"""
import argparse, fnmatch, importlib.util, json, os, re, shutil, subprocess, sys, tempfile, time
from pathlib import Path
from urllib.parse import unquote

HERE = Path(__file__).resolve().parent
REPO = Path(subprocess.run(["git", "-C", str(HERE), "rev-parse", "--show-toplevel"],
                           capture_output=True, text=True, check=True).stdout.strip())
EXCLUDE_FILE = HERE / "exclude.txt"


def load_language_gate():
    """Prose extraction + Spanish detection, owned by scripts/ci/language-check.py.

    Measuring raw file bytes here was the bug (CR-abe5ff3d): identifiers such as
    `y_rotation`, `i32_le` or `z1` are homographs of Spanish function words, so a
    token-level stopword count flagged English Rust code. The gate counts human prose
    only (markdown text outside code spans, comments, docstrings) using the CI gate's
    unambiguous word list, so both surfaces apply one rule.
    """
    path = REPO / "scripts/ci/language-check.py"
    if not path.is_file():
        sys.exit("ABORT: prose extractor missing: " + str(path))
    spec = importlib.util.spec_from_file_location("language_check", path)
    module = importlib.util.module_from_spec(spec)
    # No __pycache__: the import runs before the dirty-tree check, and a stray cache
    # directory would make the exporter abort on its own source tree.
    previous, sys.dont_write_bytecode = sys.dont_write_bytecode, True
    try:
        spec.loader.exec_module(module)
    finally:
        sys.dont_write_bytecode = previous
    return module


LANG = load_language_gate()
LANG_LANGS = LANG.PROSE_EXT | LANG.COMMENT_EXT

RETAIL_EXT = {".bsa", ".dat", ".cfg", ".bio", ".vid", ".flc", ".img", ".cif",
              ".exe", ".rsc", ".qbn", ".qrc", ".fnt", ".pal", ".sav", ".dfa"}

# English-only public surface: any Spanish-specific accent or inverted punctuation mark
# betrays untranslated prose (the letters/signs stay in SPANISH_RX below as pattern data).
SPANISH_RX = re.compile(r"[áéíóúñÁÉÍÓÚÑ¿¡]")

# The upstream MIT corpus is excluded from the language gate: it is not our prose.
LANG_SKIP = ("reference/dfu-spec/",)
RETAIL_MARKERS = [b"Temple Treasurers", b"Castle Daggerfall", b"throughout your youth"]
# User absolute paths only. The `hirundodev` handle is the public signature chosen
# by the owner (D1/D4 audit) and is NOT data to redact.
PII_PATTERNS = [re.compile(rb"/home/[A-Za-z0-9._-]+/"),
                re.compile(rb"/Users/[A-Za-z0-9._-]+/"),
                re.compile(rb"C:\\\\Users\\\\[A-Za-z0-9._-]+\\\\")]
# Declared exemptions: DFU MIT corpus (upstream attribution) and the export tool itself.
SCAN_SKIP = ("tools/public-export/",)
RETAIL_ALLOW = ("reference/dfu-spec/",)
# Third-party emails kept as MIT attribution (owner DECISION 2026-10-21):
# the DFU corpus and the NOTICE credits are NOT redacted.
EMAIL_ALLOW = ("reference/dfu-spec/", "NOTICE",
               # Synthetic fixture identity for the gate's own git invocation
               # (`-c user.email=...`, reserved TLD, not a person).
               "crates/promotion-gate/src/baseline.rs")
SECRET_PATTERNS = [
    (re.compile(rb"AKIA[0-9A-Z]{16}"), "aws-access-key"),
    (re.compile(rb"ghp_[A-Za-z0-9]{36}"), "github-token"),
    (re.compile(rb"sk-[A-Za-z0-9]{32,}"), "openai-like-key"),
    (re.compile(rb"-----BEGIN [A-Z ]*PRIVATE KEY-----"), "private-key"),
]
LIMIT_BYTES = 50 * 1024 * 1024
H1_PNG = "docs/sintergic/game/validation/m18-demo-20261021/m17-ui-fixed.png"
# Milestone tags of this repository: `M13b` is the one tagged parity follow-up, hence `[a-z]?`.
RELEASE_TAG_RX = re.compile(r"^M\d+[a-z]?-\d{4}-\d{2}-\d{2}$")

# The public surface is declared piecewise so that this file does not contain
# the literals the scanner itself looks for (0 false positives against itself).
INTERNAL_NAME = b"daggerfall-" + b"like-engine"
PUBLIC_NAME = b"daggerfall-bevy-native"
OWNER_MARKERS = [b"allan" + b"valdezdev", b"mail" + b"to:"]
EMAIL_RX = re.compile(rb"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")
LINK_RX = re.compile(r"(!?)\[([^\]]*)\]\(\s*([^)\s]+)(?:\s+\"[^\"]*\")?\s*\)")
SKIP_URL = ("http://", "https://", "#", "mail" + "to:", "ftp://")
# Deterministic commit identity; composed piecewise so as not to introduce a literal
# email into the published surface (RFC 2606 reserved domain).
COMMIT_ID = "noreply" + "@" + "example.invalid"


def patterns():
    out = []
    for raw in EXCLUDE_FILE.read_text().splitlines():
        line = raw.strip()
        if line and not line.startswith("#"):
            out.append(line)
    return out


def excluded(path, pats):
    for p in pats:
        if any(ch in p for ch in "*?["):
            if fnmatch.fnmatch(path, p) or fnmatch.fnmatch(Path(path).name, p):
                return p
        elif path == p.rstrip("/") or path.startswith(p.rstrip("/") + "/"):
            return p
    return None


def head_files():
    raw = subprocess.run(["git", "-C", str(REPO), "ls-tree", "-r", "--name-only", "-z", "HEAD"],
                         capture_output=True, check=True).stdout
    return [p.decode() for p in raw.split(b"\0") if p]


def git_dirty():
    """Entries of `git status --porcelain` (includes untracked). Empty = clean tree."""
    res = subprocess.run(["git", "-C", str(REPO), "status", "--porcelain"], capture_output=True,
                         text=True, check=True).stdout
    return [l for l in res.splitlines() if l.strip()]


def release_declaration(head, override):
    """Why this HEAD is a declared release — or abort before anything is written.

    An export is a publication act, so a clean tree is not enough: it only says the
    files match HEAD, not that the commit was ever accepted. The publisher names it in
    exactly one of three ways:
      * a milestone tag `M<n>-YYYY-MM-DD` pointing at HEAD,
      * a checked-out `release/*` branch at HEAD,
      * `--release-commit <sha>` resolving exactly to HEAD (offline verification of a
        tag or of the owner's acceptance when the tag is absent by design).
    """
    if override:
        resolved = run(["git", "-C", str(REPO), "rev-parse", "--verify", "--quiet",
                        override + "^{commit}"], REPO).stdout.strip()
        if resolved != head:
            sys.exit("ABORT: --release-commit %s does not resolve to HEAD (%s); refusing to "
                     "publish a commit that was not declared as a release." % (override, head[:12]))
        return "override:" + resolved
    tags = run(["git", "-C", str(REPO), "tag", "--points-at", "HEAD"], REPO).stdout.split()
    for tag in tags:
        if RELEASE_TAG_RX.match(tag):
            return "tag:" + tag
    branch = run(["git", "-C", str(REPO), "rev-parse", "--abbrev-ref", "HEAD"], REPO).stdout.strip()
    if branch.startswith("release/"):
        return "branch:" + branch
    sys.exit("ABORT: HEAD %s (branch %s, tags %s) is not a declared release commit.\n"
             "Publication requires one of: a milestone tag `M<n>-YYYY-MM-DD` on HEAD, a "
             "`release/*` branch, or --release-commit <sha> resolving to HEAD."
             % (head[:12], branch or "detached", ", ".join(tags) or "none"))


def head_blobs(paths):
    """Exact HEAD content for each path (a single git invocation)."""
    if not paths:
        return {}
    req = "".join("HEAD:" + p + "\n" for p in paths).encode()
    out = subprocess.run(["git", "-C", str(REPO), "cat-file", "--batch"], input=req,
                         capture_output=True, check=True).stdout
    blobs, i = {}, 0
    for p in paths:
        nl = out.index(b"\n", i)
        header = out[i:nl].split()
        if len(header) != 3:
            i = nl + 1
            continue
        size = int(header[2])
        start = nl + 1
        blobs[p] = out[start:start + size]
        i = start + size + 1
    return blobs


def run(args, cwd):
    return subprocess.run(args, cwd=cwd, capture_output=True, text=True)


def is_text(data):
    return b"\0" not in data[:8192]


def walk_text(root):
    for dirpath, dirnames, filenames in os.walk(root):
        if ".git" in dirnames:
            dirnames.remove(".git")
        for name in filenames:
            p = Path(dirpath) / name
            try:
                data = p.read_bytes()
            except OSError:
                continue
            if is_text(data):
                yield p, data


def existing_rel_paths(root):
    rels = set()
    for p in root.rglob("*"):
        if ".git" in p.relative_to(root).parts:
            continue
        rels.add(str(p.relative_to(root)).replace(os.sep, "/"))
    return rels


def mask_code_spans(line):
    """Replaces the content of `code spans` with spaces (same length)."""
    out, i, n, in_span = list(line), 0, len(line), False
    while i < n:
        if line[i] == "`":
            in_span = not in_span
            out[i] = " "
            i += 1
            continue
        if in_span and line[i] != "\n":
            out[i] = " "
        i += 1
    return "".join(out)


def parse_links(text, rel):
    """Returns [(start, end, is_image, target, label)] for markdown links outside fences/code."""
    links = []
    offset = 0
    in_fence = False
    for line in text.splitlines(keepends=True):
        stripped = line.lstrip()
        if stripped.startswith("```") or stripped.startswith("~~~"):
            in_fence = not in_fence
            offset += len(line)
            continue
        if not in_fence and "](" in line:
            masked = mask_code_spans(line)
            for m in LINK_RX.finditer(masked):
                links.append((offset + m.start(), offset + m.end(), bool(m.group(1)),
                              m.group(3), m.group(2)))
        offset += len(line)
    return links


def resolves(target, rel, existing):
    raw = unquote(target.split("#", 1)[0].split("?", 1)[0])
    if not raw:
        return True  # anchor within the same document
    base = os.path.dirname(rel)
    resolved = os.path.normpath(os.path.join(base, raw)).replace(os.sep, "/")
    if resolved == ".":
        return True
    if resolved in existing:
        return True
    prefix = resolved.rstrip("/") + "/"
    return any(e.startswith(prefix) for e in existing)


def neutralize_links(root, rels):
    """Drops links whose target is not exported; keeps the text. Deterministic."""
    touched, dropped = {}, 0
    for rel in rels:
        if not rel.endswith(".md"):
            continue
        p = root / rel
        text = p.read_bytes().decode("utf-8", "surrogateescape")
        links = [l for l in parse_links(text, rel)
                 if not any(l[3].startswith(s) for s in SKIP_URL)]
        if not links:
            continue
        out, last, changed = [], 0, 0
        for start, end, is_image, target, label in links:
            if resolves(target, rel, rels):
                continue
            out.append(text[last:start])
            if is_image:
                out.append(label)          # non-exported image: the alt text remains
            else:
                out.append(label)          # broken link: the text remains
            last = end
            changed += 1
        out.append(text[last:])
        if changed:
            p.write_bytes("".join(out).encode("utf-8", "surrogateescape"))
            touched[rel] = changed
            dropped += changed
    return touched, dropped


def broken_links(root, rels):
    bad = []
    for rel in sorted(rels):
        if not rel.endswith(".md"):
            continue
        text = (root / rel).read_bytes().decode("utf-8", "surrogateescape")
        for _, _, _, target, _label in parse_links(text, rel):
            if any(target.startswith(s) for s in SKIP_URL):
                continue
            if not resolves(target, rel, rels):
                bad.append({"file": rel, "target": target})
    return bad


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out")
    ap.add_argument("--force", action="store_true")
    ap.add_argument("--keep", action="store_true", help="do not delete the tempdir on failure")
    ap.add_argument("--release-commit", metavar="SHA",
                    help="declare the release commit: it must resolve to HEAD. Alternative to a "
                         "milestone tag `M<n>-YYYY-MM-DD` or a `release/*` branch.")
    a = ap.parse_args()

    # --- Provenance: HEAD only, and only with a clean tree, BEFORE touching the destination.
    # An export from a dirty worktree would publish edits from other lanes that are not in HEAD
    # and were never verified, and pass C would not see it either (reproduces the same tree).
    dirty = git_dirty()
    if dirty:
        sys.exit("ABORT: dirty source tree (" + str(len(dirty)) + " entries); not exporting.\n"
                 + "\n".join(dirty[:10]))
    head0 = run(["git", "-C", str(REPO), "rev-parse", "HEAD"], REPO).stdout.strip()
    # --- Publication origin (B7): clean tree + a commit declared as a release, BEFORE the destination.
    release = release_declaration(head0, a.release_commit)

    out = Path(a.out) if a.out else Path(tempfile.mkdtemp(prefix="daggerfall-public-export-"))
    if out.exists() and any(out.iterdir()):
        if not a.force:
            sys.exit(f"destination not empty: {out} (use --force or delete it)")
        shutil.rmtree(out)
    out.mkdir(parents=True, exist_ok=True)

    pats = patterns()
    files = head_files()
    keep, dropped, dropped_by = [], [], set()
    for f in files:
        p = excluded(f, pats)
        if p:
            dropped.append(f)
            dropped_by.add(p.rstrip("/"))
        else:
            keep.append(f)

    blobs = head_blobs(keep)
    for f in keep:
        dst = out / f
        dst.parent.mkdir(parents=True, exist_ok=True)
        dst.write_bytes(blobs[f])

    # --- Declared public surface (affects the export only, never the source repo) ---
    renamed = []
    for p, data in list(walk_text(out)):
        rel = str(p.relative_to(out))
        if rel.startswith(SCAN_SKIP):
            continue
        if INTERNAL_NAME in data:
            p.write_bytes(data.replace(INTERNAL_NAME, PUBLIC_NAME))
            renamed.append(rel)
    rels = existing_rel_paths(out)
    neutralized, links_removed = neutralize_links(out, rels)

    head_date = run(["git", "-C", str(REPO), "log", "-1", "--format=%cI"], REPO).stdout.strip() or "2026-10-21T00:00:00+00:00"
    env = dict(os.environ, GIT_AUTHOR_NAME="public-export", GIT_AUTHOR_EMAIL=COMMIT_ID,
               GIT_COMMITTER_NAME="public-export", GIT_COMMITTER_EMAIL=COMMIT_ID,
               GIT_AUTHOR_DATE=head_date, GIT_COMMITTER_DATE=head_date,
               GIT_CONFIG_GLOBAL="/dev/null", GIT_CONFIG_SYSTEM="/dev/null")
    def g(args):
        return subprocess.run(["git", *args], cwd=out, capture_output=True, text=True, env=env)
    g(["init", "-q", "-b", "main"])
    pathspec = out / ".export-paths"
    pathspec.write_text("\n".join(keep) + "\n")
    g(["add", "--force", "--pathspec-from-file=" + str(pathspec)])
    pathspec.unlink()
    c = g(["-c", "commit.gpgsign=false", "commit", "-q", "--no-verify",
           "-m", "chore(public): sanitized export (CR-5a5c759b)"])
    if c.returncode != 0:
        sys.exit("commit failed: " + c.stderr)

    files_out = sorted(p for p in out.rglob("*") if p.is_file() and ".git" not in p.relative_to(out).parts)
    total = sum(p.stat().st_size for p in files_out)
    rels = existing_rel_paths(out)
    checks, ok = {}, True

    def check(name, bad, detail):
        nonlocal ok
        checks[name] = {"pass": not bad, **detail}
        ok = ok and not bad

    hits = {}
    big = []
    owner_hits, upstream_emails, other_emails = [], [], []
    spanish_files = []
    lang_flagged, lang_scanned = [], 0
    for p, data in walk_text(out):
        rel = str(p.relative_to(out)).replace(os.sep, "/")
        if any(m in data for m in OWNER_MARKERS):
            owner_hits.append(rel)
        for m in EMAIL_RX.finditer(data):
            (upstream_emails if rel.startswith(EMAIL_ALLOW) or rel in EMAIL_ALLOW else other_emails).append(
                {"file": rel, "email": m.group(0).decode("utf-8", "replace")})
        if rel.startswith(SCAN_SKIP):
            continue
        if any(rx.search(data) for rx in PII_PATTERNS):
            hits.setdefault("pii", []).append(rel)
        if not rel.startswith(RETAIL_ALLOW) and any(mk in data for mk in RETAIL_MARKERS):
            hits.setdefault("retail_text", []).append(rel)
        for rx, tag in SECRET_PATTERNS:
            if rx.search(data):
                hits.setdefault("secret:" + tag, []).append(rel)
        if INTERNAL_NAME in data:
            hits.setdefault("internal_name", []).append(rel)
        text = data.decode("utf-8", "replace")
        if SPANISH_RX.search(text):
            spanish_files.append(rel)
        if rel.endswith(tuple(LANG_LANGS)) and not rel.startswith(LANG_SKIP):
            lang_scanned += 1
            prose = LANG.human_lines(rel, text)
            for block in LANG.blocks(prose):
                es, en, accents = LANG.stats(" ".join(t for _, t in block))
                if not LANG.is_spanish(es, en, accents):
                    continue
                line_no, phrase = next(((n, t.strip()) for n, t in block if t.strip()),
                                       (block[0][0], ""))
                lang_flagged.append({"file": rel, "es": es, "en": en,
                                     "ratio": round(es / (es + en), 3) if (es + en) else 0.0,
                                     "where": f"{rel}:{line_no}:{phrase[:120]}"})
    for p in files_out:
        if p.stat().st_size > LIMIT_BYTES:
            big.append(str(p.relative_to(out)))

    check("pii_paths", bool(hits.get("pii")), {"files": sorted(set(hits.get("pii", [])))[:20]})
    check("retail_text", bool(hits.get("retail_text")), {"files": sorted(set(hits.get("retail_text", [])))[:20]})
    secrets = {k.split(":", 1)[1]: sorted(set(v))[:10] for k, v in hits.items() if k.startswith("secret:")}
    if shutil.which("gitleaks"):
        r = run(["gitleaks", "detect", "--no-git", "--source", str(out), "--exit-code", "7"], out)
        check("secrets", r.returncode == 7, {"scanner": "gitleaks", "detail": secrets or None})
    elif shutil.which("trufflehog"):
        r = run(["trufflehog", "filesystem", str(out), "--json", "--no-update"], out)
        check("secrets", bool(r.stdout.strip()), {"scanner": "trufflehog", "detail": secrets or None})
    else:
        check("secrets", bool(secrets), {"scanner": "builtin", "detail": secrets or None})

    retail = [str(p.relative_to(out)) for p in files_out if p.suffix.lower() in RETAIL_EXT]
    check("retail_extensions", bool(retail), {"files": retail[:20]})
    check("large_blobs", bool(big), {"files": big[:10], "limit_mb": 50})

    ign = run(["git", "ls-files"], out).stdout.splitlines()
    ci = subprocess.run(["git", "check-ignore", "--stdin"], cwd=out, input="\n".join(ign),
                        capture_output=True, text=True)
    ignored = [x for x in ci.stdout.splitlines() if x]
    check("check_ignore", bool(ignored), {"ignored": ignored[:20]})

    remotes = run(["git", "remote"], out).stdout.strip()
    check("no_remote", bool(remotes), {"remotes": remotes or None})
    commits = run(["git", "rev-list", "--count", "HEAD"], out).stdout.strip()
    check("single_commit", commits != "1", {"commits": int(commits or 0)})
    leaked_prefixes = sorted({p for f in rels for p in [excluded(f, pats)] if p})
    check("excluded_prefixes_absent", bool(leaked_prefixes), {"leaked": leaked_prefixes})
    check("h1_retail_capture_absent", (out / H1_PNG).exists(), {"path": H1_PNG})

    # --- Publication gate criteria ---
    check("owner_email_absent", bool(owner_hits), {"files": sorted(set(owner_hits))[:20]})
    check("emails_other_absent", bool(other_emails),
          {"count": len(other_emails), "files": other_emails[:20],
           "upstream_allowlisted": len(upstream_emails)})
    bad_links = broken_links(out, rels)
    check("internal_links_resolved", bool(bad_links), {"broken": len(bad_links), "detail": bad_links[:20]})
    check("public_name_absent_internal", bool(hits.get("internal_name")),
          {"files": sorted(set(hits.get("internal_name", [])))[:20]})
    readme = (out / "README.md").read_text(encoding="utf-8", errors="surrogateescape").splitlines()
    check("readme_title_public", not readme or readme[0].strip() != "# " + PUBLIC_NAME.decode(),
          {"first_line": readme[0] if readme else None})
    lic = (out / "LICENSE").read_text(encoding="utf-8", errors="surrogateescape")
    check("license_holder", "Copyright (c) 2026 HirundoDev" not in lic or bool(EMAIL_RX.search(lic.encode())),
          {"holder": "Copyright (c) 2026 HirundoDev" in lic})
    notice = (out / "NOTICE").read_text(encoding="utf-8", errors="surrogateescape") if (out / "NOTICE").exists() else ""
    check("notice_holder", "Copyright (c) 2026 HirundoDev" not in notice,
          {"holder": "Copyright (c) 2026 HirundoDev" in notice})
    root_docs = sorted(f for f in rels if "/" not in f and (
        fnmatch.fnmatch(f, "MASTER-PLAN-100J-*.md")
        or fnmatch.fnmatch(f, "dfu-improvement-catalog-*.md")
        or fnmatch.fnmatch(f, "prototype-gap-*.md")))
    check("internal_root_docs_absent", bool(root_docs), {"found": root_docs})
    es_md = sorted(f for f in rels if f.endswith(".es.md"))
    check("excluded_es_md_absent", bool(es_md), {"found": es_md[:20]})
    check("spanish_stopwords_absent", bool(lang_flagged),
          {"files": len(lang_flagged), "scanned": lang_scanned,
           "thresholds": {"min_es_tokens": LANG.MIN_ES, "max_es_ratio": LANG.MAX_RATIO,
                          "min_accents": LANG.MIN_ACCENT},
           "sample": [x["where"] for x in lang_flagged[:10]]})

    head1 = run(["git", "-C", str(REPO), "rev-parse", "HEAD"], REPO).stdout.strip()
    if head1 != head0:
        sys.exit("ABORT: HEAD changed during the export (" + head0[:12] + " -> " + head1[:12] + "); rerun with a stable HEAD.")
    explained = set(renamed) | set(neutralized)
    unexplained = [f for f in keep
                   if f not in explained and (out / f).read_bytes() != blobs[f]]
    check("exported_matches_head_blobs", bool(unexplained),
          {"files": len(keep), "unexplained": unexplained[:20],
           "declared_transforms": {"name_substitution": len(renamed), "links": len(neutralized)}})
    check("source_tree_clean", False, {"dirty_entries": 0})
    check("head_stable_at_export", head1 != head0, {"head_start": head0, "head_end": head1})

    sha = run(["git", "rev-parse", "HEAD"], out).stdout.strip()
    report = {
        "generated": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "source_repo": "<repo>", "source_head": run(["git", "-C", str(REPO), "rev-parse", "HEAD"], REPO).stdout.strip(),
        "release": {"head": head0, "declared_by": release},
        "out": str(out), "commit": sha, "commit_date": head_date,
        "files_exported": len(files_out), "bytes_exported": total,
        "files_dropped": len(dropped), "dropped_by_rule": sorted(dropped_by),
        "surface": {"name_substituted_files": len(renamed),
                    "links_neutralized": links_removed,
                    "links_neutralized_by_file": neutralized,
                    "spanish_surface": {"files": len(spanish_files),
                                        "sample": sorted(spanish_files)[:10]},
                    "language": {"scanned": lang_scanned, "flagged_count": len(lang_flagged),
                                 "accents_files": len(spanish_files), "flagged": lang_flagged}},
        "emails": {"owner": sorted(set(owner_hits)), "other": other_emails,
                   "upstream_allowlisted": len(upstream_emails)},
        "checks": checks, "verdict": "PASS" if ok else "FAIL",
    }
    (out.parent / (out.name + ".report.json")).write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps(report, indent=2, ensure_ascii=False))
    if not ok and not a.keep and not a.out:
        print(f"(tempdir kept: {out})", file=sys.stderr)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
