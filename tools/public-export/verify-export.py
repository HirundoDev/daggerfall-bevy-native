#!/usr/bin/env python3
"""Independent verification of a public export (passes A-E, ORDER CR-5a5c759b / CR-5c3261ee).

Usage:
  python3 tools/public-export/verify-export.py --export <dir>
  python3 tools/public-export/verify-export.py --export <dir> --regen-dir <dir2>   # full pass C

Deliberately does not import export-public.py: each pass reimplements its own rules so that a
generator bug cannot validate itself.

A: tree scan (owner/upstream/other emails, internal links, PII, retail, secrets, name).
B: git object scan (every blob in history, not only the tree).
C: regeneration from scratch in a second directory + byte-by-byte comparison.
D: file-by-file review against the allowlist derived from the source repo's HEAD.
E: internal-reference metric (files, lines, functional, non-functional) with a data allowlist;
   a functional reference that is not allowlisted fails the pass *with the number*.
Exit 0 = A/B/C/D/E PASS.
"""
import argparse, fnmatch, hashlib, json, os, re, shutil, subprocess, sys, tempfile, time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = Path(subprocess.run(["git", "-C", str(HERE), "rev-parse", "--show-toplevel"],
                           capture_output=True, text=True, check=True).stdout.strip())
EXPORT_TOOL = HERE / "export-public.py"
EXCLUDE_FILE = HERE / "exclude.txt"
# Pass E tolerances: one `path:line<TAB>reason` row per functional reference that the public
# tree is allowed to keep. No wildcards: a moved or deleted line rots the row and fails E.
ALLOWLIST_FILE = HERE / "internal-refs-allowlist.txt"
# Internal change-request id recorded in the report (pass D). It is supplied at run time with
# `--cr` on purpose: this file ships in the public export, so it carries no internal id.
CR = ""

RETAIL_EXT = {".bsa", ".dat", ".cfg", ".bio", ".vid", ".flc", ".img", ".cif",
              ".exe", ".rsc", ".qbn", ".qrc", ".fnt", ".pal", ".sav", ".dfa"}
RETAIL_MARKERS = [b"Temple Treasurers", b"Castle Daggerfall", b"throughout your youth"]
RETAIL_ALLOW = ("reference/dfu-spec/",)
# Declared exemption: the scanner itself carries the markers as pattern literals.
RETAIL_SCAN_SKIP = ("tools/public-export/",)
EMAIL_ALLOW = ("reference/dfu-spec/", "NOTICE",
               # Synthetic fixture identity for the gate's own git invocation
               # (`-c user.email=...`, reserved TLD, not a person).
               "crates/promotion-gate/src/baseline.rs")
INTERNAL_NAME = b"daggerfall-" + b"like-engine"
PUBLIC_NAME = b"daggerfall-bevy-native"
OWNER_MARKERS = [b"allan" + b"valdezdev", b"mail" + "to:".encode()]
EMAIL_RX = re.compile(rb"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")
PII_RX = re.compile(rb"/home/[A-Za-z0-9._-]+/|/Users/[A-Za-z0-9._-]+/|C:\\\\Users\\\\[A-Za-z0-9._-]+\\\\")
SECRET_RX = [
    (re.compile(rb"AKIA[0-9A-Z]{16}"), "aws-access-key"),
    (re.compile(rb"ghp_[A-Za-z0-9]{36}"), "github-token"),
    (re.compile(rb"sk-[A-Za-z0-9]{32,}"), "openai-like-key"),
    (re.compile(rb"-----BEGIN [A-Z ]*PRIVATE KEY-----"), "private-key"),
]
LIMIT = 50 * 1024 * 1024
MD_LINK = re.compile(r"!?\[([^\]]*)\]\(\s*([^)\s]+?)(?:\s+\"[^\"]*\")?\s*\)")


def sh(args, cwd=None, **kw):
    return subprocess.run(args, cwd=cwd, capture_output=True, **kw)


def text_of(p):
    data = p.read_bytes()
    return data if b"\0" not in data[:8192] else None


def files_of(root):
    out = []
    for dirpath, dirnames, filenames in os.walk(root):
        if ".git" in dirnames:
            dirnames.remove(".git")
        for n in filenames:
            p = Path(dirpath) / n
            out.append(str(p.relative_to(root)).replace(os.sep, "/"))
    return sorted(out)


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def exclude_patterns():
    return [l.strip() for l in EXCLUDE_FILE.read_text().splitlines()
            if l.strip() and not l.strip().startswith("#")]


def is_excluded(rel, pats):
    for p in pats:
        if any(c in p for c in "*?["):
            if fnmatch.fnmatch(rel, p) or fnmatch.fnmatch(os.path.basename(rel), p):
                return p
        elif rel == p.rstrip("/") or rel.startswith(p.rstrip("/") + "/"):
            return p
    return None


# ---------------------------------------------------------------- pass A
def markdown_without_code(data):
    """Drops fences and inline code so only real links count."""
    text = data.decode("utf-8", "surrogateescape")
    text = re.sub(r"```.*?```", "", text, flags=re.S)
    return re.sub(r"`[^`\n]*`", "", text)


def broken_links(root, rels):
    bad = []
    existing = set(rels)
    for rel in rels:
        if not rel.endswith(".md"):
            continue
        text = markdown_without_code((root / rel).read_bytes())
        for m in MD_LINK.finditer(text):
            target = m.group(2)
            if target.startswith(("http://", "https://", "#", "ftp://")) or target.startswith(
                    "mail" + "to:"):
                continue
            raw = target.split("#", 1)[0].split("?", 1)[0]
            if not raw:
                continue
            resolved = os.path.normpath(os.path.join(os.path.dirname(rel), raw)).replace(os.sep, "/")
            if resolved in existing or resolved == "." or any(e.startswith(resolved + "/") for e in existing):
                continue
            bad.append({"file": rel, "target": target})
    return bad


def pass_a(root):
    rels = files_of(root)
    owner, upstream, other = [], [], []
    pii, retail, secrets, internal = [], [], [], []
    big = []
    for rel in rels:
        p = root / rel
        if p.stat().st_size > LIMIT:
            big.append(rel)
        data = text_of(p)
        if data is None:
            continue
        if any(m in data for m in OWNER_MARKERS):
            owner.append(rel)
        for m in EMAIL_RX.finditer(data):
            rec = {"file": rel, "email": m.group(0).decode("utf-8", "replace")}
            (upstream if rel.startswith(EMAIL_ALLOW) or rel in EMAIL_ALLOW else other).append(rec)
        if PII_RX.search(data):
            pii.append(rel)
        if not rel.startswith(RETAIL_ALLOW) and not rel.startswith(RETAIL_SCAN_SKIP) \
                and any(mk in data for mk in RETAIL_MARKERS):
            retail.append(rel)
        for rx, tag in SECRET_RX:
            if rx.search(data):
                secrets.append({"file": rel, "kind": tag})
        if INTERNAL_NAME in data:
            internal.append(rel)
    retail_ext = [r for r in rels if os.path.splitext(r)[1].lower() in RETAIL_EXT]
    bad = broken_links(root, rels)
    readme = (root / "README.md").read_text(encoding="utf-8", errors="surrogateescape").splitlines() \
        if (root / "README.md").exists() else []
    lic = (root / "LICENSE").read_bytes() if (root / "LICENSE").exists() else b""
    notice = (root / "NOTICE").read_bytes() if (root / "NOTICE").exists() else b""
    root_docs = [r for r in rels if "/" not in r and (
        r.startswith("MASTER-PLAN-100J-") or r.startswith("dfu-improvement-catalog-")
        or r.startswith("prototype-gap-"))]
    res = {
        "files": len(rels),
        "bytes": sum((root / r).stat().st_size for r in rels),
        "emails_owner": sorted(set(owner)),
        "emails_upstream_allowlisted": len(upstream),
        "emails_other": other,
        "broken_links": bad,
        "pii_paths": sorted(set(pii)),
        "retail_text": sorted(set(retail)),
        "retail_scan_skip_declared": list(RETAIL_SCAN_SKIP),
        "retail_extensions": retail_ext,
        "secrets": secrets,
        "large_blobs": big,
        "internal_name_files": sorted(set(internal)),
        "readme_first_line": readme[0] if readme else None,
        "license_holder": b"Copyright (c) 2026 HirundoDev" in lic,
        "license_has_email": bool(EMAIL_RX.search(lic)),
        "notice_holder": b"Copyright (c) 2026 HirundoDev" in notice,
        "internal_root_docs": root_docs,
    }
    res["verdict"] = "PASS" if (
        not res["emails_owner"] and not res["emails_other"] and not res["broken_links"]
        and not res["pii_paths"] and not res["retail_text"] and not res["retail_extensions"]
        and not res["secrets"] and not res["large_blobs"] and not res["internal_name_files"]
        and res["readme_first_line"] == "# daggerfall-bevy-native"
        and res["license_holder"] and not res["license_has_email"] and res["notice_holder"]
        and not res["internal_root_docs"]) else "FAIL"
    return res


# ---------------------------------------------------------------- pass B
def pass_b(root):
    rev = sh(["git", "-C", str(root), "rev-list", "--objects", "--all"], text=True)
    entries = []
    for line in rev.stdout.splitlines():
        if not line:
            continue
        parts = line.split(" ", 1)
        entries.append((parts[0], parts[1] if len(parts) > 1 else ""))
    bad_paths, bad_blobs = [], []
    blobs = 0
    for sha, path in entries:
        if not path:
            continue
        if path.startswith(("docs/sintergic/", ".sintergic/")) or is_excluded(path, exclude_patterns()):
            bad_paths.append(path)
        if os.path.splitext(path)[1].lower() in RETAIL_EXT:
            bad_paths.append(path)
        typ = sh(["git", "-C", str(root), "cat-file", "-t", sha], text=True).stdout.strip()
        if typ != "blob":
            continue
        blobs += 1
        data = sh(["git", "-C", str(root), "cat-file", "blob", sha]).stdout
        if any(m in data for m in OWNER_MARKERS) or INTERNAL_NAME in data or PII_RX.search(data):
            bad_blobs.append(path)
        if not path.startswith(EMAIL_ALLOW) and path not in EMAIL_ALLOW and EMAIL_RX.search(data):
            bad_blobs.append(path + " (email outside allowlist)")
        if not path.startswith(RETAIL_ALLOW) and not path.startswith(RETAIL_SCAN_SKIP) \
                and any(mk in data for mk in RETAIL_MARKERS):
            bad_blobs.append(path + " (retail text)")
    counts = sh(["git", "-C", str(root), "rev-list", "--count", "HEAD"], text=True).stdout.strip()
    res = {
        "objects": len(entries), "blobs": blobs,
        "paths_outside_allowlist": sorted(set(bad_paths)),
        "blobs_with_forbidden_content": sorted(set(bad_blobs)),
        "commits": int(counts or 0),
        "remotes": sh(["git", "-C", str(root), "remote"], text=True).stdout.strip(),
    }
    res["verdict"] = "PASS" if (not res["paths_outside_allowlist"]
                                and not res["blobs_with_forbidden_content"]
                                and res["commits"] == 1 and not res["remotes"]) else "FAIL"
    return res


# ---------------------------------------------------------------- pass C
def pass_c(root, regen_dir):
    regen = Path(regen_dir)
    # B7: the exporter refuses to write unless the source HEAD is a declared release. Pass C
    # regenerates from the same HEAD, so it declares it explicitly (`--release-commit`); the
    # alternative (a tag or a `release/*` branch) is the publisher's call, not the verifier's.
    head = sh(["git", "-C", str(REPO), "rev-parse", "HEAD"], text=True).stdout.strip()
    out = sh([sys.executable, str(EXPORT_TOOL), "--out", str(regen), "--force",
              "--release-commit", head], text=True)
    if out.returncode != 0:
        return {"verdict": "FAIL", "reason": "regeneration failed", "stderr": out.stderr[-2000:],
                "report": out.stdout[-2000:]}
    a_files, b_files = files_of(root), files_of(regen)
    only_a = sorted(set(a_files) - set(b_files))
    only_b = sorted(set(b_files) - set(a_files))
    diffs = [f for f in a_files if f in set(b_files) and sha256(root / f) != sha256(regen / f)]
    sha_a = sh(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).stdout.strip()
    sha_b = sh(["git", "-C", str(regen), "rev-parse", "HEAD"], text=True).stdout.strip()
    tree_a = sh(["git", "-C", str(root), "rev-parse", "HEAD^{tree}"], text=True).stdout.strip()
    tree_b = sh(["git", "-C", str(regen), "rev-parse", "HEAD^{tree}"], text=True).stdout.strip()
    head_now = sh(["git", "-C", str(REPO), "rev-parse", "HEAD"], text=True).stdout.strip()
    res = {
        "regen_dir": str(regen), "files_a": len(a_files), "files_b": len(b_files),
        "only_in_export": only_a, "only_in_regen": only_b, "content_diffs": diffs,
        "commit_export": sha_a, "commit_regen": sha_b,
        "tree_export": tree_a, "tree_regen": tree_b,
        "source_head_at_verify": head_now,
    }
    # ORDER criterion: "git diff --stat between exports = 0". The commit sha depends on the source
    # HEAD date and stops being comparable if another lane commits during verification;
    # the invariant is the tree plus the bytes.
    res["verdict"] = "PASS" if (tree_a == tree_b and not only_a and not only_b and not diffs) else "FAIL"
    res["commit_sha_equal"] = sha_a == sha_b
    return res


# ---------------------------------------------------------------- pass D
def expected_files():
    raw = sh(["git", "-C", str(REPO), "ls-tree", "-r", "--name-only", "-z", "HEAD"]).stdout
    pats = exclude_patterns()
    return [p for p in raw.decode().split("\0") if p and not is_excluded(p, pats)]


def category(rel):
    if rel.startswith("crates/"):
        return "product-code"
    if rel.startswith("reference/dfu-spec/"):
        return "third-party-spec-mit"
    if rel.startswith("docs/sintergic/") or rel.startswith(".sintergic/"):
        return "INTERNAL (forbidden)"
    if rel.startswith("docs/"):
        return "public-docs"
    if rel.startswith(("scripts/", ".github/")):
        return "tooling-ci"
    if rel.startswith("tools/public-export/"):
        return "export-tooling"
    if rel.startswith("saves/"):
        return "INTERNAL (forbidden)"
    return "root-entry-or-legal"


def head_blob(rel):
    return sh(["git", "-C", str(REPO), "show", "HEAD:" + rel]).stdout


def line_neutralized(line, rel, existing):
    """Drops links whose target does not exist in the export (pass D's own implementation)."""
    masked = re.sub(r"`[^`\n]*`", lambda m: " " * len(m.group(0)), line)
    edits = []
    for m in MD_LINK.finditer(masked):
        target = m.group(2)
        if target.startswith(("http://", "https://", "#", "ftp://")) or target.startswith("mail" + "to:"):
            continue
        raw = target.split("#", 1)[0].split("?", 1)[0]
        if not raw:
            continue
        resolved = os.path.normpath(os.path.join(os.path.dirname(rel), raw)).replace(os.sep, "/")
        if resolved in existing or resolved == "." or any(e.startswith(resolved + "/") for e in existing):
            continue
        edits.append((m.start(), m.end(), m.group(1)))
    for start, end, label in reversed(edits):
        line = line[:start] + label + line[end:]
    return line


def md_derived(src, rel, existing):
    """True if `src` (HEAD) differs from the export only by public name + non-exported links."""
    internal = INTERNAL_NAME.decode()
    public = PUBLIC_NAME.decode()
    sl = src.decode("utf-8", "surrogateescape").splitlines(keepends=True)
    out, in_fence = [], False
    for line in sl:
        if line.lstrip().startswith(("```", "~~~")):
            in_fence = not in_fence
            out.append(line)
            continue
        out.append(line if in_fence else line_neutralized(line, rel, existing))
    text = "".join(out)
    # the generator applies the name to ALL lines (including inside fences)
    return text.replace(internal, public)


def pass_d(root):
    expected = expected_files()
    actual = files_of(root)
    existing = set(actual)
    missing = sorted(set(expected) - set(actual))
    extra = sorted(set(actual) - set(expected))
    rows, unverified, verbatim, transformed = [], [], 0, 0
    for rel in actual:
        data = (root / rel).read_bytes()
        src = head_blob(rel) if rel in set(expected) else b""
        if data == src:
            origin = "verbatim"
            verbatim += 1
        elif data == src.replace(INTERNAL_NAME, PUBLIC_NAME):
            origin = "name-substituted"
            transformed += 1
        elif rel.endswith(".md") and data == md_derived(src, rel, existing).encode("utf-8", "surrogateescape"):
            origin = "name+links"
            transformed += 1
        else:
            origin = "UNEXPLAINED"
            unverified.append(rel)
        rows.append({"path": rel, "category": category(rel), "bytes": len(data),
                     "sha256": hashlib.sha256(data).hexdigest(), "origin": origin})
    manifest = json.dumps([{k: r[k] for k in ("path", "sha256")} for r in rows],
                          sort_keys=True).encode()
    res = {
        "expected_from_head": len(expected), "actual": len(actual),
        "missing": missing, "extra": extra, "unexplained_content": unverified,
        "verbatim": verbatim, "transformed": transformed,
        "categories": {},
        "manifest_sha256": hashlib.sha256(manifest).hexdigest(),
        "reviewer": "og-orquestador/PublishGate",
        "cr": CR,
        "signed_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "files": rows,
    }
    for r in rows:
        res["categories"][r["category"]] = res["categories"].get(r["category"], 0) + 1
    res["verdict"] = "PASS" if not missing and not extra and not unverified else "FAIL"
    return res


# ---------------------------------------------------------------- pass E
# Internal-reference metric (ORDER CR-5c3261ee). Self-contained on purpose: the token list is
# repeated here rather than imported from the generator, so a generator bug cannot validate
# itself — the same property every other pass keeps.
#
# `docs/sintergic/`, `.sintergic/`, `.omp/`, `.opencode/` and `promotion-ledger` are the internal
# paths that do NOT travel in the export. Their mentions are classified deterministically:
#
#   non-functional  prose in `.md`; a comment in `.rs` (`//`, `///`, `/* */`) or in
#                   `.py`/`.sh`/`.toml` (`#`); an exclusion pattern inside `exclude.txt`; a row of
#                   the tolerance data file; any other non-code extension (data/config).
#   functional      a mention in `.rs`/`.py`/`.sh`/`.toml` outside a comment (a path constant,
#                   `include_str!`, `include_bytes!`, `fs::read*`, `File::open`, `Path::new`,
#                   `join(...)`, a `startswith(...)` prefix, a scanner pattern...), or ANY
#                   mention in a file under `tests/` or inside a `#[cfg(test)]` item.
#
# A functional mention must be tolerated by `internal-refs-allowlist.txt` (`path:line<TAB>reason`).
# The metric is printed on every run, PASS included, so it cannot be hidden.
INTERNAL_REF_RX = re.compile(r"docs/sintergic/|\.sintergic/|\.omp/|\.opencode/|promotion-ledger")
CODE_EXT = (".rs", ".py", ".sh", ".toml")
# Declared data files: their internal paths are exclusion patterns / tolerance rows, never reads.
# (`.gitignore` is treated the same way by basename below.)
DATA_NONFUNCTIONAL = ("tools/public-export/exclude.txt",
                      "tools/public-export/internal-refs-allowlist.txt")
TEST_RX = re.compile(r"(^|/)tests/")


def rust_comment_ranges(lines):
    """Per line, the [start, end) columns covered by a Rust comment.

    Walks the file so `//` inside a string literal is not a comment and `/* */` spans lines.
    Raw strings (`r#"..."#`) are treated as ordinary strings from their first quote; they do not
    appear in the paths this pass measures.
    """
    out, block_from = [], None
    for line in lines:
        ranges, i, n = [], 0, len(line)
        if block_from is not None:
            ranges.append((block_from, -1))  # closed below once `*/` is found
        while i < n:
            if block_from is not None:
                j = line.find("*/", i)
                if j < 0:
                    i = n
                    break
                ranges[-1] = (ranges[-1][0], j + 2)
                block_from = None
                i = j + 2
                continue
            ch = line[i]
            if ch == '"':
                i += 1
                while i < n:
                    if line[i] == "\\":
                        i += 2
                        continue
                    if line[i] == '"':
                        i += 1
                        break
                    i += 1
                continue
            if line.startswith("//", i):
                ranges.append((i, n))
                i = n
                continue
            if line.startswith("/*", i):
                block_from = i
                i += 2
                continue
            i += 1
        if block_from is not None:
            ranges[-1] = (ranges[-1][0], n)
        out.append([(a, n if b < 0 else b) for a, b in ranges])
    return out


def hash_comment_ranges(lines):
    """Per line, the [start, end) columns covered by a `#` comment (`.py`, `.sh`, `.toml`)."""
    out = []
    for line in lines:
        cut, quote, i, n = None, None, 0, len(line)
        while i < n:
            ch = line[i]
            if quote:
                if ch == "\\":
                    i += 2
                    continue
                if ch == quote:
                    quote = None
                i += 1
                continue
            if ch in "'\"":
                quote = ch
                i += 1
                continue
            if ch == "#":
                cut = i
                break
            i += 1
        out.append([(cut, n)] if cut is not None else [])
    return out


def cfg_test_lines(lines):
    """0-based indexes of the lines living inside a `#[cfg(test)]` item."""
    out, pending, depth = set(), False, 0
    for idx, line in enumerate(lines):
        if depth:
            out.add(idx)
            depth = max(0, depth + line.count("{") - line.count("}"))
            continue
        if pending:
            out.add(idx)
            if "{" in line:
                depth = max(0, line.count("{") - line.count("}"))
                pending = False
            elif line.strip().endswith(";"):
                pending = False
            continue
        if "#[cfg(test)]" in line:
            out.add(idx)
            rest = line.split("#[cfg(test)]", 1)[1]
            if "{" in rest:
                depth = max(0, rest.count("{") - rest.count("}"))
            elif not rest.strip().endswith(";"):
                pending = True
    return out


def internal_ref_items(root):
    """Every internal-path occurrence in the exported tree, with its classification."""
    items = []
    for rel in files_of(root):
        data = text_of(root / rel)
        if data is None:
            continue
        text = data.decode("utf-8", "surrogateescape")
        if not INTERNAL_REF_RX.search(text):
            continue
        lines = text.splitlines()
        ext = os.path.splitext(rel)[1].lower()
        if ext == ".rs":
            ranges = rust_comment_ranges(lines)
        elif ext in (".py", ".sh", ".toml"):
            ranges = hash_comment_ranges(lines)
        else:
            ranges = [[] for _ in lines]
        test_block = cfg_test_lines(lines) if ext == ".rs" else set()
        is_data = rel in DATA_NONFUNCTIONAL or rel.rsplit("/", 1)[-1] == ".gitignore"
        is_test = bool(TEST_RX.search(rel))
        is_prose = ext == ".md"
        for idx, line in enumerate(lines):
            for m in INTERNAL_REF_RX.finditer(line):
                col = m.start()
                if is_data or (is_prose and not is_test):
                    functional = False
                elif is_test or idx in test_block:
                    functional = True
                elif ext in CODE_EXT:
                    functional = not any(a <= col < b for a, b in ranges[idx])
                else:
                    functional = False
                items.append({"path": rel, "line": idx + 1, "column": col + 1,
                              "text": line.strip()[:200], "functional": functional})
    return items


def allowlist_rows():
    """({(path, line): reason}, [malformed rows]) from `path:line<TAB>reason`."""
    rows, errors = {}, []
    if not ALLOWLIST_FILE.exists():
        return rows, errors
    for num, raw in enumerate(ALLOWLIST_FILE.read_text(encoding="utf-8").splitlines(), 1):
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        if "\t" not in raw:
            errors.append("line %d: missing tab separator" % num)
            continue
        key, reason = raw.split("\t", 1)
        path, sep, line_no = key.rpartition(":")
        if not sep or not line_no.isdigit() or not path:
            errors.append("line %d: key %r is not `path:line`" % (num, key))
            continue
        if not reason.strip():
            errors.append("line %d: empty reason for %s" % (num, key))
            continue
        if any(c in path for c in "*?["):
            errors.append("line %d: wildcard in path %r" % (num, path))
            continue
        rows[(path, int(line_no))] = reason.strip()
    return rows, errors


def pass_e(root):
    items = internal_ref_items(root)
    allow, allow_errors = allowlist_rows()
    by_line = {}
    for it in items:
        by_line.setdefault((it["path"], it["line"]), []).append(it)
    functional = sorted(k for k, v in by_line.items() if any(x["functional"] for x in v))
    allowlisted = [k for k in functional if k in allow]
    unallowlisted = [k for k in functional if k not in allow]
    stale = [k for k in allow if k not in set(functional)]
    rows = []
    for key in sorted(by_line):
        path, line = key
        is_func = key in set(functional)
        rows.append({"path": path, "line": line, "functional": is_func,
                     "disposition": "allowlisted" if key in allow else
                                    ("functional" if is_func else "non_functional"),
                     "text": by_line[key][0]["text"]})
    res = {
        "files": len({k[0] for k in by_line}),
        "lines": len(by_line),
        "occurrences": len(items),
        "functional": len(functional),
        "non_functional": len(by_line) - len(functional),
        "allowlisted": len(allowlisted),
        "unallowlisted": [{"path": p, "line": l, "text": by_line[(p, l)][0]["text"]}
                          for p, l in unallowlisted],
        "stale_allowlist": [{"path": p, "line": l, "reason": allow[(p, l)]} for p, l in stale],
        "allowlist_errors": allow_errors,
        "allowlist": str(ALLOWLIST_FILE),
        "items": rows,
    }
    ok = not unallowlisted and not stale and not allow_errors
    res["verdict"] = "PASS" if ok else "FAIL"
    if not ok:
        res["reason"] = ("%d functional internal reference(s) not allowlisted, %d stale "
                         "allowlist row(s), %d malformed allowlist row(s)"
                         % (len(unallowlisted), len(stale), len(allow_errors)))
    return res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--export", required=True)
    ap.add_argument("--regen-dir")
    ap.add_argument("--report")
    ap.add_argument("--skip-c", action="store_true")
    ap.add_argument("--cr", default="", help="internal change-request id recorded in the report "
                                            "(supplied at run time; never part of the export)")
    a = ap.parse_args()
    global CR
    CR = a.cr
    root = Path(a.export).resolve()
    if not (root / ".git").exists():
        sys.exit(f"not a git export: {root}")

    A = pass_a(root)
    B = pass_b(root)
    if a.skip_c:
        C = {"verdict": "SKIP"}
    else:
        regen = a.regen_dir or tempfile.mkdtemp(prefix="daggerfall-public-regen-")
        C = pass_c(root, regen)
    D = pass_d(root)
    E = pass_e(root)

    report = {"cr": CR, "export": str(root), "generated": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
              "source_head": sh(["git", "-C", str(REPO), "rev-parse", "HEAD"], text=True).stdout.strip(),
              "A_tree": A, "B_gitobjects": B, "C_regeneration": C, "D_file_by_file": D,
              "E_internal_refs": E}
    report["verdict"] = "PASS" if all(report[k]["verdict"] == "PASS" for k in
                                      ("A_tree", "B_gitobjects", "C_regeneration", "D_file_by_file",
                                       "E_internal_refs")) else "FAIL"
    dest = Path(a.report) if a.report else root.parent / (root.name + ".verify.json")
    dest.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    summary = {k: report[k] for k in ("verdict", "export", "source_head", "generated")}
    for k in ("A_tree", "B_gitobjects", "C_regeneration", "D_file_by_file", "E_internal_refs"):
        # `files` is the metric itself on pass E; only the per-occurrence list is elided.
        summary[k] = {kk: vv for kk, vv in report[k].items() if kk != "items"}
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    # The internal-reference metric is printed unconditionally: a PASS must not hide it.
    print("E_internal_refs: files=%d lines=%d functional=%d non_functional=%d allowlisted=%d "
          "occurrences=%d verdict=%s"
          % (E["files"], E["lines"], E["functional"], E["non_functional"], E["allowlisted"],
             E["occurrences"], E["verdict"]))
    if E["verdict"] != "PASS":
        print("E_internal_refs FAIL: %d functional reference(s) not allowlisted, %d stale row(s), "
              "%d malformed row(s)" % (len(E["unallowlisted"]), len(E["stale_allowlist"]),
                                       len(E["allowlist_errors"])))
        for row in E["unallowlisted"][:20]:
            print("  not allowlisted: %s:%d  %s" % (row["path"], row["line"], row["text"]))
        for row in E["stale_allowlist"][:20]:
            print("  stale allowlist: %s:%d  (%s)" % (row["path"], row["line"], row["reason"]))
        for err in E["allowlist_errors"][:20]:
            print("  allowlist error: %s" % err)
    print(f"report: {dest}")
    sys.exit(0 if report["verdict"] == "PASS" else 1)


if __name__ == "__main__":
    main()
