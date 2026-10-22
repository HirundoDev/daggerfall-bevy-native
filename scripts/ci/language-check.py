#!/usr/bin/env python3
"""language-check.py - English-only gate for the exportable surface (CI scan).

Policy: docs/i18n/POLICY.md and CONTRIBUTING.md ("The English gate"). The exportable
surface is the set of files the public export can ship: everything git knows
about that tools/public-export/exclude.txt does not exclude. The gate and the
publication therefore share one source of truth for what is published.

What the gate reads (human text only):
  *.md                          prose outside fenced code, inline code, HTML comments
  *.rs                          // line comments and /* */ block comments
  *.py                          docstrings (module, class, function), read with
                                ast, plus whole-line # comments
  *.sh *.bash *.toml
  *.yml *.yaml                  whole-line comments (#); .toml also reads the
                                `description = "..."` value (a sentence, not code)
  Every other file (JSON, lockfiles, images, binaries) holds no prose.

Why not a word blacklist: identifiers, paths, probe IDs and English words that
coincide with Spanish function words ("no", "sin", "son") make a plain match
useless. The gate measures, per contiguous prose block, the density of
unambiguous Spanish function words against English ones, and treats
Spanish-specific letters and inverted punctuation as a second signal:

  es >= 2 and es / (es + en) > 0.15
  or two or more accented/inverted characters and at least one function word

The decision is density, not the presence of a word: Spanish prose carries
several function words per sentence, while English prose that borrows a term
carries none of the unambiguous ones above. A single borrowed word is not
enough to fail a file.

Known limit, stated rather than hidden: a code comment short enough to hold a
single function word ("// el caso") stays under the density floor. The gate is
a guardrail against prose, and the publication gate re-checks the exported tree.

Declared exemptions (reported, never gating):
  docs/sintergic/**      internal engineering record, not published
  *.es.md                local translations, excluded from the export
  reference/dfu-spec/**  vendored MIT corpus, never edited
  quotes and identifiers code spans, fenced code and citation tokens
                         (PROBE-*, INV-*, REQ-*, ALL-*) are stripped before counting

Usage:
  python3 scripts/ci/language-check.py [--root DIR] [--verbose] [--list-surface]
  exit 0 = English-only surface - exit 1 = Spanish prose found - exit 2 = usage
"""
import argparse, ast, fnmatch, os, re, subprocess, sys
from pathlib import Path

HERE = Path(__file__).resolve().parent


def repo_root():
    out = subprocess.run(["git", "-C", str(HERE), "rev-parse", "--show-toplevel"],
                         capture_output=True, text=True)
    return Path(out.stdout.strip()) if out.returncode == 0 and out.stdout.strip() else HERE.parent


ROOT = repo_root()
EXCLUDE_FILE = ROOT / "tools/public-export/exclude.txt"

PROSE_EXT = {".md", ".markdown"}
COMMENT_EXT = {".rs", ".sh", ".bash", ".py", ".toml", ".yml", ".yaml"}
EXEMPT_PREFIX = ("docs/sintergic/", "reference/dfu-spec/")
EXEMPT_SUFFIX = (".es.md",)
COMMENT_HASH_EXT = {".sh", ".bash", ".py", ".toml", ".yml", ".yaml"}
WALK_SKIP_DIRS = {".git", "target", "saves", "node_modules", "__pycache__", ".venv", "venv"}

TOKEN_RX = re.compile("[a-záéíóúüñ]+")
ACCENT_RX = re.compile("[áéíóúüñÁÉÍÓÚÜÑ¿¡]")
FENCE_RX = re.compile(r"^\s*(```|~~~)")
INLINE_CODE_RX = re.compile(r"`[^`]*`")
MD_LINK_RX = re.compile(r"\]\([^)]*\)")
HTML_TAG_RX = re.compile(r"<[^>]+>")
URL_RX = re.compile(r"(?:https?|ftp)://\S+")
STRING_LIT_RX = re.compile(r'"(?:[^"\\]|\\.)*"|\'(?:[^\'\\]|\\.)*\'')
TOML_DESC_RX = re.compile(r'^description\s*=\s*(.+)$')

# Unambiguous Spanish function words only: English homographs ("no", "sin",
# "son", "se", "es", "en", "un", "su") are deliberately absent,
# so the density signal cannot be produced by English prose alone. Accents are
# folded before the lookup, so the list is written unaccented.
ES_WORDS = set("""
el la los las de del al que para con por una unos unas como pero este esta estos
estas sus mas tambien desde entre sobre cuando donde todo todos toda todas hay
estan fue fueron han muy cada otro otra otros otras porque aunque mientras antes
despues siempre nunca aqui asi esto eso esa esos esas nuestro nuestra puede
pueden hace hacer sido segun durante mediante ante bajo contra hacia hasta
""".split())
EN_WORDS = set("""
the of and to in is for with on that this are be as at by or from it not we you
was were will has have can all if when there which more than but its into does do
""".split())

MIN_ES, MAX_RATIO, MIN_ACCENT = 2, 0.15, 2


# ── Scope: which files are on the exportable surface ──────────────────────
def patterns(exclude_file):
    """Same parsing as tools/public-export/export-public.py:patterns()."""
    out = []
    if exclude_file.is_file():
        for raw in exclude_file.read_text(encoding="utf-8").splitlines():
            line = raw.strip()
            if line and not line.startswith("#"):
                out.append(line)
    return out


def excluded(path, pats):
    """Same semantics as tools/public-export/export-public.py:excluded()."""
    for p in pats:
        if any(ch in p for ch in "*?["):
            if fnmatch.fnmatch(path, p) or fnmatch.fnmatch(Path(path).name, p):
                return p
        elif path == p.rstrip("/") or path.startswith(p.rstrip("/") + "/"):
            return p
    return None


def is_exempt(path):
    return path.endswith(EXEMPT_SUFFIX) or path.startswith(EXEMPT_PREFIX)


def git_files(root):
    out = subprocess.run(
        ["git", "-C", str(root), "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        capture_output=True)
    if out.returncode != 0:
        return None
    return [p.decode("utf-8", "replace") for p in out.stdout.split(b"\0") if p]


def walk_files(root):
    found = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(d for d in dirnames if d not in WALK_SKIP_DIRS)
        for name in filenames:
            found.append(str(Path(dirpath, name).relative_to(root)).replace(os.sep, "/"))
    return found


def surface(root, exclude_file):
    files = git_files(root)
    if files is None:
        files = walk_files(root)
    pats = patterns(exclude_file)
    return sorted({f for f in files if not excluded(f, pats) and not is_exempt(f)})


# ── Human text extraction ─────────────────────────────────────────────────
def md_lines(text):
    out, fence, in_comment = [], False, False
    for no, line in enumerate(text.splitlines(), 1):
        s = line.strip()
        if FENCE_RX.match(s):
            fence = not fence
            continue
        if fence:
            continue
        if in_comment:
            if "-->" in s:
                in_comment = False
            continue
        if s.startswith("<!--"):
            if "-->" not in s:
                in_comment = True
            continue
        s = INLINE_CODE_RX.sub(" ", s)
        s = MD_LINK_RX.sub("] ", s)
        s = URL_RX.sub(" ", s)
        s = HTML_TAG_RX.sub(" ", s)
        out.append((no, s))
    return out


def comment_lines(text, ext):
    """Whole-line comments, block comments and .toml description values."""
    out, in_block = [], False
    for no, line in enumerate(text.splitlines(), 1):
        s = line.strip()
        if in_block:
            body, closed = (s.split("*/", 1)[0], True) if "*/" in s else (s, False)
            in_block = not closed
            out.append((no, body))
            continue
        if s.startswith("/*"):
            body = s[2:]
            if "*/" in body:
                out.append((no, body.split("*/", 1)[0]))
            else:
                out.append((no, body))
                in_block = True
            continue
        if ext in COMMENT_HASH_EXT:
            if s.startswith("#") and not s.startswith("#!"):
                out.append((no, s.lstrip("#")))
            elif ext == ".toml":
                m = TOML_DESC_RX.match(s)
                if m:
                    out.append((no, m.group(1).strip().strip('"\'')))
            continue
        if s.startswith("//"):
            out.append((no, s.lstrip("/")))
            continue
        bare = STRING_LIT_RX.sub('""', s)
        cut = bare.find("//")
        if cut > 0 and bare[cut - 1] != ":":
            out.append((no, bare[cut + 2:]))
    return out


def py_lines(text):
    """Python docstrings (module, class, function) plus whole-line # comments.

    A docstring is prose that ships: it is read with ast, never by matching
    quotes, so string literals that hold data (format strings, pattern tables,
    fixture samples) are not mistaken for sentences.
    """
    out = []
    try:
        tree = ast.parse(text)
    except SyntaxError:
        tree = None
    if tree is not None:
        for node in ast.walk(tree):
            if not isinstance(node, (ast.Module, ast.ClassDef, ast.FunctionDef,
                                     ast.AsyncFunctionDef)):
                continue
            doc = ast.get_docstring(node, clean=False)
            if not doc:
                continue
            first = node.body[0].lineno
            for offset, line in enumerate(doc.splitlines()):
                out.append((first + offset, line))
    out.extend(comment_lines(text, ".py"))
    out.sort(key=lambda item: item[0])
    return out


def human_lines(rel, text):
    ext = Path(rel).suffix.lower()
    if ext in PROSE_EXT:
        return md_lines(text)
    if ext == ".py":
        return py_lines(text)
    return comment_lines(text, ext)


def blocks(lines):
    """Groups contiguous line numbers; a gap of two or more starts a new block."""
    grouped, current, last = [], [], None
    for no, txt in lines:
        if last is not None and no > last + 1:
            grouped.append(current)
            current = []
        current.append((no, txt))
        last = no
    if current:
        grouped.append(current)
    return grouped


# ── Detection ─────────────────────────────────────────────────────────────
FOLD = str.maketrans("áéíóúüñÁÉÍÓÚÜÑ", "aeiouunAEIOUUN")


def words(text):
    return [t.translate(FOLD) for t in TOKEN_RX.findall(text.lower())]


def stats(text):
    toks = words(text)
    es = sum(1 for t in toks if t in ES_WORDS)
    en = sum(1 for t in toks if t in EN_WORDS)
    return es, en, len(ACCENT_RX.findall(text))


def is_spanish(es, en, accents):
    ratio = es / (es + en) if (es + en) else 0.0
    return (es >= MIN_ES and ratio > MAX_RATIO) or (accents >= MIN_ACCENT and es >= 1)


def scan(root, exclude_file):
    files = surface(root, exclude_file)
    scanned, out_of_scope, hits = 0, 0, []
    for rel in files:
        ext = Path(rel).suffix.lower()
        if ext not in PROSE_EXT and ext not in COMMENT_EXT:
            out_of_scope += 1
            continue
        try:
            data = (root / rel).read_bytes()
        except OSError:
            continue  # deleted in the working tree since git listed it
        if b"\0" in data[:8192]:
            out_of_scope += 1
            continue
        text = data.decode("utf-8", "replace")
        lines = human_lines(rel, text)
        scanned += 1
        for block in blocks(lines):
            es, en, accents = stats(" ".join(t for _, t in block))
            if is_spanish(es, en, accents):
                line_no, phrase = next(((n, t.strip()) for n, t in block if t.strip()),
                                       (block[0][0], ""))
                hits.append({"file": rel, "line": line_no, "es": es, "en": en,
                             "accent": accents, "phrase": phrase[:110]})
    return files, scanned, out_of_scope, hits


def main(argv):
    ap = argparse.ArgumentParser(description="English-only gate for the exportable surface.")
    ap.add_argument("--root", default=str(ROOT), help="tree to scan (default: this repository)")
    ap.add_argument("--exclude", default="", help="exclude list to reuse (default: <root>/tools/public-export/exclude.txt)")
    ap.add_argument("--verbose", action="store_true", help="print per-file Spanish/English counts")
    ap.add_argument("--list-surface", action="store_true", help="print the scanned surface and exit")
    args = ap.parse_args(argv)

    root = Path(args.root).resolve()
    if not root.is_dir():
        print(f"language-check: not a directory: {root}", file=sys.stderr)
        return 2
    exclude_file = Path(args.exclude) if args.exclude else root / "tools/public-export/exclude.txt"

    if args.list_surface:
        for rel in surface(root, exclude_file):
            print(rel)
        return 0

    files, scanned, out_of_scope, hits = scan(root, exclude_file)

    print("language-check: CI-ENGLISH-SURFACE-001 . CR-6f9f4742-ee4a-4f7f-9684-4a2cf7a80edb")
    print(f"root: {root}")
    print(f"surface_files: {len(files)} (scanned={scanned} no_prose={out_of_scope})")
    print("exempt: docs/sintergic/ **  *.es.md  reference/dfu-spec/ **  quotes and identifiers")
    print(f"thresholds: es>={MIN_ES} and ratio>{MAX_RATIO}, or accents>={MIN_ACCENT} with es>=1")

    if args.verbose:
        for rel in files:
            ext = Path(rel).suffix.lower()
            if ext not in PROSE_EXT and ext not in COMMENT_EXT:
                continue
            try:
                text = (root / rel).read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            lines = human_lines(rel, text)
            es, en, accents = stats("\n".join(t for _, t in lines))
            ratio = es / (es + en) if (es + en) else 0.0
            print(f"  scan {rel}: es={es} en={en} ratio={ratio:.3f} accents={accents}")

    print(f"hits: {len(hits)}")
    for hit in hits:
        print(f"  HIT {hit['file']}:{hit['line']} (es={hit['es']} en={hit['en']} "
              f"accents={hit['accent']}) {hit['phrase']}")

    if hits:
        print("language-check: FAIL (translate the flagged prose to English; "
              "translations live in *.es.md, which is local-only)")
        return 1
    print("language-check: PASS")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except BrokenPipeError:  # `| head` must not print a traceback
        os.dup2(os.open(os.devnull, os.O_WRONLY), sys.stdout.fileno())
        sys.exit(0)
