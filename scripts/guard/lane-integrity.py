#!/usr/bin/env python3
"""lane-integrity.py — auditor de cierre de lane, model-agnóstico (CR-51d25f1e).

No conoce al ejecutor: recibe un CR o un rango de commits y dicta PASS/FAIL sobre
el CIERRE tal como quedó en la historia. No lee reportes de la lane ni confía en
su prosa; re-deriva de git y de los ficheros de cierre.

Uso:
  python3 scripts/guard/lane-integrity.py --cr CR-ca78eb76
  python3 scripts/guard/lane-integrity.py --cr CR-ca78eb76-a8a9-4789-a841-1b27bf428110
  python3 scripts/guard/lane-integrity.py --commits d897d94..d1ee654
  python3 scripts/guard/lane-integrity.py --cr CR-... --json
  python3 scripts/guard/lane-integrity.py --selftest           # 7 casos, sobre ESTE repo
  python3 scripts/guard/lane-integrity.py --selftest --case mixto

Comprobaciones (códigos tipados; un solo FAIL hace FAIL al cierre):
  (a) A-MIXTO            un commit toca a la vez implementación (`crates/**`, `src/**`)
                         y gate (`scripts/*-001.py`, `scripts/*gate*.py`).
  (b) GATE_POSTERIOR     el gate de la lane NO existía en el árbol del commit ANTERIOR
                         al primer commit de implementación del mismo CR (nació después
                         o en el mismo commit). Si la lane no toca ningún gate, basta
                         con que OTRO gate preexista (gate escrito por otra lane).
      GATE_AUSENTE       hay implementación y no hay ningún gate previo identificable.
  (c) STATUS_SIN_QA      el HH-1 declara `status` por encima de `candidato` y no existe
                         fila de QA independiente (autor ≠ autor del HH-1) con veredicto.
  (d) EVIDENCIA_AUSENTE  una ruta citada como evidencia no existe en la revisión auditada.
      EVIDENCIA_SHA      el sha256 declarado en el INDEX no coincide con los bytes del
                         fichero citado en esa misma revisión.
  (e) PUSH_DETECTADO     un commit auditado es alcanzable desde `refs/remotes/*`.
      TAG_EN_RANGO       un commit auditado es alcanzable desde `refs/tags/*`.

Salida: matriz de 5 filas + lista de hallazgos; exit 0 = PASS, 1 = FAIL, 2 = uso/error.

LÍMITES DECLARADOS (lo que este instrumento NO puede verificar):
  · Si el gate es DÉBIL por dentro: un gate que existe, corre y siempre da verde pasa
    aquí. La calidad del gate es del auditor, no de este script.
  · Si el gate mide lo que dice medir (fidelidad contrato↔fila): fuera de alcance.
  · Si el veredicto de la fila de QA es honesto: aquí solo se comprueba que existe una
    fila de OTRO autor con un token de veredicto; no se juzga su contenido.
  · Rutas citadas como evidencia de OTRO repo, o borradas después del cierre, salen
    como ausentes en la revisión auditada (la revisión se declara con `--at`).
  · Un push cuyo ref remoto fue borrado después no deja rastro en `refs/remotes/*`.
  · Un commit que se reescribió (amend/rebase) no es visible: se audita la historia
    vigente, no la que existió.
  · `status` se lee del frontmatter del HH-1 y del INDEX: si ambos callan, no se opina.
  · Commits que no llevan el CR en el mensaje quedan FUERA del conjunto auditado en
    modo `--cr` (ese hueco lo señala `SIN_COMMITS`/la fila de conteo, no se adivina).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

# ── contrato de patrones ─────────────────────────────────────────────────────

# gate = instrumento de cierre (mismo universo que guard-paths.sh, D2b/D4)
RE_GATE = re.compile(r"^scripts/(?:[^/]*-001\.py|[^/]*gate[^/]*\.py|closure-gate\.py)$")
# implementación = producto
RE_IMPL = re.compile(r"^(?:crates|src)/")
# ruta citable como evidencia
RE_EVID_PATH = re.compile(r"^(?:docs|crates|src|scripts|sides|assets|\.sintergic)/[A-Za-z0-9._/-]+$")
RE_SHA_LINE_SUFFIX = re.compile(r":\d+(?:-\d+)?$")
RE_BACKTICK = re.compile(r"`([^`\n]+)`")
RE_AUTHOR = re.compile(r"^\s*(?:agent|agente|auditor|qa_agent)\s*:\s*(.+?)\s*$", re.MULTILINE)
RE_JSON_AUTHOR = re.compile(r'"(?:agent|agente|auditor|qa_agent)"\s*:\s*"([^"]+)"')
RE_VERDICT = re.compile(r"\b(PASS|ACEPTA(?:DO)?|APROBADO|VERDE|veredicto\s*:\s*\S+)\b", re.IGNORECASE)
RE_FM_BLOCK = re.compile(r"\A---\s*\n(.*?)\n---\s*(?:\n|\Z)", re.DOTALL)
RE_FM_FIELD = re.compile(r"^([a-z_]+)\s*:\s*(.+?)\s*$", re.MULTILINE)

# estado ≤ candidato (contrato §1b + §4). Cualquier otro valor por encima exige QA.
STATUS_BAJOS = {
    "candidato", "candidate", "partial", "parcial", "blocked", "bloqueado",
    "escalated", "escalado", "open", "abierto", "running", "en_curso", "wip",
    "pending", "pendiente", "none", "n/a", "na",
}
STATUS_ALTOS = {
    "done", "pass", "passed", "complete", "completed", "completo", "cerrado",
    "closed", "aceptado", "accepted", "ok", "aprobado", "verde", "green",
    "listo", "terminado",
}

DEFAULT_INDEX = "docs/sintergic/handoffs/INDEX.jsonl"
DEFAULT_HANDOFFS = "docs/sintergic/handoffs"
DEFAULT_VALIDATION = "docs/sintergic/validation"

EXIT_PASS, EXIT_FAIL, EXIT_USO = 0, 1, 2


# ── git plumbing ─────────────────────────────────────────────────────────────

def git(repo: Path, *args: str, stdin: bytes | None = None, env: dict | None = None):
    """Ejecuta git y devuelve (rc, stdout-bytes, stderr-texto). Nunca aborta."""
    full_env = dict(os.environ)
    if env:
        full_env.update(env)
    proc = subprocess.run(
        ["git", "-C", str(repo), *args],
        input=stdin,
        capture_output=True,
        env=full_env,
    )
    return proc.returncode, proc.stdout, proc.stderr.decode("utf-8", "replace")


def git_text(repo: Path, *args: str, env: dict | None = None) -> str:
    rc, out, err = git(repo, *args, env=env)
    if rc != 0:
        return ""
    return out.decode("utf-8", "replace")


def rev_parse(repo: Path, rev: str) -> str:
    rc, out, _ = git(repo, "rev-parse", "--verify", "--quiet", rev)
    return out.decode().strip() if rc == 0 else ""


def exists_at(repo: Path, rev: str, path: str) -> bool:
    rc, _, _ = git(repo, "cat-file", "-e", f"{rev}:{path}")
    return rc == 0


def bytes_at(repo: Path, rev: str, path: str) -> bytes | None:
    rc, out, _ = git(repo, "show", f"{rev}:{path}")
    return out if rc == 0 else None


def commit_paths(repo: Path, sha: str) -> list[str]:
    out = git_text(repo, "show", "--no-renames", "--name-only", "--format=", sha)
    return [line.strip() for line in out.splitlines() if line.strip()]


def first_add_commit(repo: Path, rev: str, path: str) -> str:
    """sha del commit que AÑADIÓ `path` (el más antiguo de los `A`), o ''."""
    out = git_text(repo, "log", "--diff-filter=A", "--format=%H", rev, "--", path)
    lines = [line.strip() for line in out.splitlines() if line.strip()]
    return lines[-1] if lines else ""


def is_ancestor(repo: Path, anc: str, desc: str) -> bool:
    rc, _, _ = git(repo, "merge-base", "--is-ancestor", anc, desc)
    return rc == 0


def refs_containing(repo: Path, sha: str) -> list[str]:
    out = git_text(
        repo, "for-each-ref", f"--contains={sha}", "--format=%(refname)",
        "refs/remotes", "refs/tags",
    )
    return [line.strip() for line in out.splitlines() if line.strip()]


def head_sha(repo: Path) -> str:
    return rev_parse(repo, "HEAD")


# ── hallazgos ────────────────────────────────────────────────────────────────

class Finding:
    __slots__ = ("code", "check", "severity", "detail")

    def __init__(self, code: str, check: str, severity: str, detail: str):
        self.code = code
        self.check = check
        self.severity = severity  # FAIL | WARN | NOTA
        self.detail = detail

    def as_dict(self) -> dict:
        return {"code": self.code, "check": self.check,
                "severity": self.severity, "detail": self.detail}

    def __str__(self) -> str:
        return f"[{self.code}] {self.detail}"


def cr_tokens(text: str) -> list[str]:
    return re.findall(r"(?<![0-9A-Za-z_-])CR-[0-9A-Za-z]+(?:-[0-9A-Za-z]+)*(?![0-9A-Za-z_-])", text)


def cr_short(cr: str) -> str:
    """Id corto comparable: `CR-<8hex>-<resto>` -> `<8hex>`.

    Un id corto SIN prefijo (`8cf48b73`, la forma en que el capitán nombra una lane
    en su ORDER) YA ES el id corto: se devuelve tal cual. Sin esto `--cr 8cf48b73`
    no resolvía ningún commit y el auditor emitía SIN_COMMITS/FAIL sobre una lane
    que sí tenía commits: el FAIL no medía la lane y ESCONDÍA sus hallazgos reales.
    """
    cr = (cr or "").strip()
    if not cr:
        return ""
    partes = cr.split("-")
    if partes[0].upper() == "CR":
        return partes[1].lower() if len(partes) > 1 else ""
    return partes[0].lower()


def cr_match(given: str, other: str) -> bool:
    """Compara CR corto/largo. Si ambos llevan id corto y coinciden, es el mismo CR:
    sin esto, un CR con el sufijo UUID mal escrito resolvía commits (por el id corto)
    pero no las filas del INDEX, y el HH-1 quedaba invisible (falso PASS)."""
    if not given or not other:
        return False
    if other == given or other.startswith(given) or given.startswith(other):
        return True
    corto_g, corto_o = cr_short(given), cr_short(other)
    return bool(corto_g) and corto_g == corto_o


# ── resolución del conjunto auditado ─────────────────────────────────────────

def resolve_cr_commits(repo: Path, cr: str, rev: str) -> list[str]:
    """Commits de `rev` cuyo mensaje lleva el CR como TOKEN delimitado, en orden
    topológico inverso (padre→hijo). Sin `--all`: se audita la historia vigente."""
    out = git_text(repo, "log", "--format=%H%x1f%s%x1f%b%x1e", rev)
    ordered = [line.strip() for line in out.splitlines() if line.strip()]
    hit = []
    for chunk in ordered:
        parts = chunk.split("\x1f")
        sha = parts[0].strip()
        text = " ".join(parts[1:])
        if any(cr_match(cr, tok) for tok in cr_tokens(text)):
            hit.append(sha)
    hit.reverse()
    return hit


def resolve_range(repo: Path, spec: str) -> tuple[str, list[str], str]:
    """`a..b` → (a, commits en orden padre→hijo, b)."""
    if ".." not in spec:
        raise ValueError("--commits exige el formato a..b")
    left, right = spec.split("..", 1)
    left = left or "HEAD"
    right = right or "HEAD"
    a = rev_parse(repo, left)
    b = rev_parse(repo, right)
    if not a or not b:
        raise ValueError(f"revisión no resoluble: {spec}")
    out = git_text(repo, "rev-list", "--reverse", f"{a}..{b}")
    return a, [line.strip() for line in out.splitlines() if line.strip()], b


# ── lectura de HH-1 / INDEX ──────────────────────────────────────────────────

def rel_path(repo: Path, raw: str) -> str:
    """Normaliza una ruta citada a repo-relativa. '' si queda fuera del repo.

    El INDEX de este target guarda rutas ABSOLUTAS; el cuerpo de los HH-1 usa
    rutas relativas. Ambas formas son la misma evidencia: se comparan igual.
    """
    path = (raw or "").strip().strip('"').strip("'")
    if path.startswith("./"):
        path = path[2:]
    if os.path.isabs(path):
        try:
            return Path(os.path.realpath(path)).relative_to(os.path.realpath(repo)).as_posix()
        except ValueError:
            return ""
    return path


def read_text_at(repo: Path, rev: str, path: str) -> str | None:
    data = bytes_at(repo, rev, path)
    if data is None:
        disk = repo / path
        if disk.is_file():
            return disk.read_text(encoding="utf-8", errors="replace")
        return None
    return data.decode("utf-8", "replace")


def load_index_rows(repo: Path, rev: str, index_path: str) -> list[dict]:
    raw = read_text_at(repo, rev, index_path)
    if not raw:
        return []
    rows = []
    for line in raw.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(obj, dict):
            rows.append(obj)
    return rows


def parse_frontmatter(text: str) -> dict:
    m = RE_FM_BLOCK.match(text)
    if not m:
        return {}
    fields = {}
    for key, value in RE_FM_FIELD.findall(m.group(1)):
        fields[key.strip()] = value.strip()
    return fields


def handoff_files_for_cr(repo: Path, rev: str, cr: str, rows: list[dict],
                         handoffs_dir: str) -> list[str]:
    """HH-1 del CR: los que declara el INDEX + los que aparecen en el directorio."""
    found: list[str] = []
    for row in rows:
        if not cr_match(cr, str(row.get("cr", ""))):
            continue
        path = rel_path(repo, str(row.get("path") or ""))
        if path and path not in found:
            found.append(path)
    disk_dir = repo / handoffs_dir
    if disk_dir.is_dir():
        for candidate in sorted(disk_dir.glob("*.md")):
            rel = candidate.relative_to(repo).as_posix()
            if rel in found:
                continue
            text = read_text_at(repo, rev, rel)
            if not text:
                continue
            fm = parse_frontmatter(text)
            if cr_match(cr, fm.get("cr", "")) or cr_match(cr, Path(rel).name):
                found.append(rel)
    return found


# ── (c) fila de QA independiente ─────────────────────────────────────────────

def author_of(text: str) -> str:
    m = RE_AUTHOR.search(text) or RE_JSON_AUTHOR.search(text)
    return m.group(1).strip() if m else ""


def find_independent_qa(repo: Path, cr: str, forbidden_author: str,
                        validation_dir: str) -> tuple[bool, str]:
    """Busca una fila de QA con veredicto y autor ≠ autor del HH-1.

    Universo: ficheros under `docs/sintergic/validation/` en el árbol de trabajo
    (declarado: es la vista «en disco»). Una fila cuenta si (i) menciona el CR,
    (ii) contiene un token de veredicto y (iii) declara un autor distinto.
    """
    base = repo / validation_dir
    if not base.is_dir():
        return False, f"sin directorio {validation_dir}"
    doomed = forbidden_author.strip().lower()
    sin_autor: list[str] = []
    for path in sorted(base.rglob("*")):
        if not path.is_file():
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if not any(cr_match(cr, tok) for tok in cr_tokens(text)):
            continue
        if not RE_VERDICT.search(text):
            continue
        author = author_of(text)
        rel = path.relative_to(repo).as_posix()
        if not author:
            if rel not in sin_autor:
                sin_autor.append(rel)
            continue
        if author.strip().lower() == doomed:
            continue
        return True, f"{rel} (autor {author})"
    detalle = "ninguna fila de QA con autor distinto y veredicto"
    if sin_autor:
        detalle += ("; candidatas descartadas por no declarar autor: "
                    + ", ".join(sin_autor[:3]))
    return False, detalle


# ── comprobaciones ───────────────────────────────────────────────────────────

def check_mixto(repo: Path, commits: list[str], findings: list[Finding]) -> str:
    malos = []
    for sha in commits:
        paths = commit_paths(repo, sha)
        gates = [p for p in paths if RE_GATE.match(p)]
        impls = [p for p in paths if RE_IMPL.match(p)]
        if gates and impls:
            malos.append(f"{sha[:12]} gate={','.join(gates)} impl={','.join(impls)}")
    for detail in malos:
        findings.append(Finding(
            "A-MIXTO", "a", "FAIL",
            f"commit con gate y implementación en el mismo acto: {detail}"))
    return "FAIL" if malos else "PASS"


def check_gate_orden(repo: Path, commits: list[str], at_rev: str,
                     findings: list[Finding]) -> str:
    impl_shas, lane_gates = [], []
    for sha in commits:
        paths = commit_paths(repo, sha)
        if any(RE_IMPL.match(p) for p in paths) and sha not in impl_shas:
            impl_shas.append(sha)
        for p in paths:
            if RE_GATE.match(p) and p not in lane_gates:
                lane_gates.append(p)
    if not impl_shas:
        findings.append(Finding(
            "SIN_IMPL", "b", "NOTA",
            "la lane no toca implementación: regla (b) no aplica"))
        return "N-A"
    first = impl_shas[0]
    parent = rev_parse(repo, f"{first}^")
    if not parent:
        # commit raíz: no hay «anterior» posible
        findings.append(Finding(
            "GATE_POSTERIOR", "b", "FAIL",
            f"{first[:12]} es commit raíz de la historia: no existe revisión anterior "
            f"donde pudiera preexistir el gate"))
        return "FAIL"
    if lane_gates:
        veredicto = "PASS"
        for gate in lane_gates:
            if not exists_at(repo, parent, gate):
                intro = first_add_commit(repo, at_rev, gate) or "?"
                findings.append(Finding(
                    "GATE_POSTERIOR", "b", "FAIL",
                    f"gate {gate} no existía en {parent[:12]} (padre del primer commit "
                    f"de implementación {first[:12]}); introducido en {intro[:12]}"))
                veredicto = "FAIL"
                continue
            intro = first_add_commit(repo, at_rev, gate)
            if intro and not is_ancestor(repo, intro, first):
                findings.append(Finding(
                    "GATE_POSTERIOR", "b", "FAIL",
                    f"gate {gate} introducido en {intro[:12]}, que NO es ancestro de "
                    f"{first[:12]}"))
                veredicto = "FAIL"
        return veredicto
    # la lane no toca gates: el gate lo escribió otra lane ⇒ debe preexistir alguno
    candidatos = [p for p in git_text(repo, "ls-files", "scripts").splitlines()
                  if p.strip() and RE_GATE.match(p.strip())]
    previos = [p for p in candidatos if exists_at(repo, parent, p)]
    if not previos:
        findings.append(Finding(
            "GATE_AUSENTE", "b", "FAIL",
            f"implementación en {first[:12]} sin ningún gate preexistente en "
            f"{parent[:12]} (la lane no aporta gate y no hay gate de otra lane)"))
        return "FAIL"
    findings.append(Finding(
        "GATE_EXTERNO", "b", "NOTA",
        f"la lane no toca gates; el gate preexiste en {parent[:12]}: "
        f"{', '.join(previos[:3])}{'…' if len(previos) > 3 else ''}"))
    return "PASS"


def check_status(repo: Path, cr: str, at_rev: str, rows: list[dict],
                 handoffs: list[str], validation_dir: str,
                 findings: list[Finding]) -> str:
    if not handoffs:
        # hay commits (check_status solo se llama con el conjunto no vacío) pero ningún
        # HH-1 con este CR: C6 exige file de handoff por lane cerrada ⇒ no es un aviso
        findings.append(Finding(
            "HH1_AUSENTE", "c", "FAIL",
            f"sin HH-1 localizable para {cr} en {at_rev[:12]}: la lane tiene commits "
            f"pero ningún file de handoff con su CR (C6) — no hay estado que auditar"))
        return "FAIL"
    veredicto = "PASS"
    for rel in handoffs:
        text = read_text_at(repo, at_rev, rel)
        if text is None:
            findings.append(Finding(
                "HH1_AUSENTE", "c", "FAIL",
                f"el INDEX cita {rel} y no existe en {at_rev[:12]}"))
            veredicto = "FAIL"
            continue
        fm = parse_frontmatter(text)
        status = (fm.get("status") or "").strip()
        author = (fm.get("agent") or "").strip()
        low = status.lower()
        if not status:
            findings.append(Finding(
                "STATUS_SIN_DECLARAR", "c", "WARN", f"{rel}: frontmatter sin `status`"))
            continue
        if low in STATUS_BAJOS:
            continue
        if low in STATUS_ALTOS:
            ok, why = find_independent_qa(repo, cr, author, validation_dir)
            if ok:
                findings.append(Finding(
                    "STATUS_CON_QA", "c", "NOTA",
                    f"{rel}: status `{status}` respaldado por fila de QA {why}"))
            else:
                findings.append(Finding(
                    "STATUS_SIN_QA", "c", "FAIL",
                    f"{rel}: status `{status}` (autor {author or '?'}) excede `candidato` "
                    f"y no hay fila de QA independiente — {why}"))
                veredicto = "FAIL"
            continue
        findings.append(Finding(
            "STATUS_DESCONOCIDO", "c", "WARN",
            f"{rel}: status `{status}` fuera del vocabulario del contrato "
            f"(≤ candidato salvo QA independiente)"))
    return veredicto


def check_evidencia(repo: Path, cr: str, at_rev: str, rows: list[dict],
                    handoffs: list[str], index_path: str,
                    findings: list[Finding]) -> str:
    veredicto = "PASS"
    # 1) filas del INDEX: path + sha256 declarado
    for row in rows:
        if cr and not cr_match(cr, str(row.get("cr", ""))):
            continue
        rel = rel_path(repo, str(row.get("path") or ""))
        if not rel:
            continue
        declared = str(row.get("sha256") or "").strip().lower()
        data = bytes_at(repo, at_rev, rel)
        if data is None:
            disco = repo / rel
            if disco.is_file():
                findings.append(Finding(
                    "EVIDENCIA_NO_VERSIONADA", "d", "WARN",
                    f"{index_path}: fila {row.get('id', '?')} cita {rel}, que existe en "
                    f"disco pero NO está versionada en {at_rev[:12]} (evidencia volátil)"))
                data = disco.read_bytes()
            else:
                findings.append(Finding(
                    "EVIDENCIA_AUSENTE", "d", "FAIL",
                    f"{index_path}: fila {row.get('id', '?')} cita {rel}, ausente en "
                    f"{at_rev[:12]} y en disco"))
                veredicto = "FAIL"
                continue
        if declared:
            real = hashlib.sha256(data).hexdigest()
            if real != declared:
                findings.append(Finding(
                    "EVIDENCIA_SHA", "d", "FAIL",
                    f"{index_path}: fila {row.get('id', '?')} declara sha256 {declared[:16]}… "
                    f"para {rel} y los bytes de {at_rev[:12]} dan {real[:16]}…"))
                veredicto = "FAIL"
    # 2) rutas citadas en el cuerpo de los HH-1 (backticks)
    for rel_hh in handoffs:
        text = read_text_at(repo, at_rev, rel_hh)
        if text is None:
            continue
        vistos: list[str] = []
        for lineno, line in enumerate(text.splitlines(), start=1):
            for raw in RE_BACKTICK.findall(line):
                token = RE_SHA_LINE_SUFFIX.sub("", raw.strip())
                if any(ch in token for ch in "*{}<>$…"):
                    continue
                if not RE_EVID_PATH.match(token):
                    continue
                if token in vistos:
                    continue
                token = rel_path(repo, token)
                if not token or token in vistos:
                    continue
                vistos.append(token)
                if exists_at(repo, at_rev, token):
                    continue
                # `.exists()` y no `.is_file()`: un artefacto de lane puede ser un
                # DIRECTORIO (`.sintergic/artifacts/<lane>/`). Con `.is_file()` esa
                # cita salía EVIDENCIA_AUSENTE/FAIL aunque el directorio estuviera en
                # disco — falso positivo por diseño, no defecto de la lane.
                if (repo / token).exists():
                    findings.append(Finding(
                        "EVIDENCIA_NO_VERSIONADA", "d", "WARN",
                        f"{rel_hh}:{lineno} cita `{token}`, presente en disco "
                        f"(fichero o directorio) pero NO versionada en {at_rev[:12]}"))
                    continue
                findings.append(Finding(
                    "EVIDENCIA_AUSENTE", "d", "FAIL",
                    f"{rel_hh}:{lineno} cita `{token}`, ausente en {at_rev[:12]} y en disco"))
                veredicto = "FAIL"
    return veredicto


def check_push_tag(repo: Path, commits: list[str], findings: list[Finding]) -> str:
    veredicto = "PASS"
    for sha in commits:
        for ref in refs_containing(repo, sha):
            if ref.startswith("refs/remotes/"):
                findings.append(Finding(
                    "PUSH_DETECTADO", "e", "FAIL",
                    f"{sha[:12]} es alcanzable desde {ref} (push fuera de contrato)"))
                veredicto = "FAIL"
            elif ref.startswith("refs/tags/"):
                findings.append(Finding(
                    "TAG_EN_RANGO", "e", "FAIL",
                    f"{sha[:12]} es alcanzable desde {ref} (tag fuera de contrato)"))
                veredicto = "FAIL"
    return veredicto


# ── auditoría completa ───────────────────────────────────────────────────────

def run_audit(repo: Path, cr: str | None, commits: list[str], at_rev: str,
              index_path: str, handoffs_dir: str, validation_dir: str,
              hh1_override: str | None = None) -> dict:
    findings: list[Finding] = []
    if not commits:
        findings.append(Finding(
            "SIN_COMMITS", "0", "FAIL",
            "el conjunto auditado está vacío: no hay nada que auditar (¿CR mal escrito "
            "o commits sin el CR en el mensaje?)"))
        matriz = {"a": "N-A", "b": "N-A", "c": "N-A", "d": "N-A", "e": "N-A"}
    else:
        rows = load_index_rows(repo, at_rev, index_path)
        handoffs = ([hh1_override] if hh1_override
                    else handoff_files_for_cr(repo, at_rev, cr or "", rows, handoffs_dir))
        matriz = {
            "a": check_mixto(repo, commits, findings),
            "b": check_gate_orden(repo, commits, at_rev, findings),
            "c": check_status(repo, cr or "", at_rev, rows, handoffs, validation_dir, findings),
            "d": check_evidencia(repo, cr or "", at_rev, rows, handoffs, index_path, findings),
            "e": check_push_tag(repo, commits, findings),
        }
    unicos: list[Finding] = []
    vistos_clave: set[tuple[str, str]] = set()
    for f in findings:
        clave = (f.code, f.detail)
        if clave in vistos_clave:
            continue
        vistos_clave.add(clave)
        unicos.append(f)
    findings = unicos
    fallos = [f for f in findings if f.severity == "FAIL"]
    veredicto = "FAIL" if fallos else "PASS"
    return {
        "veredicto": veredicto if commits else "FAIL",
        "cr": cr,
        "revision": at_rev,
        "commits": commits,
        "matriz": matriz,
        "hallazgos": [f.as_dict() for f in findings],
        "fallos": len(fallos),
        "avisos": len([f for f in findings if f.severity == "WARN"]),
    }


def render(result: dict, stream=sys.stdout) -> None:
    etiqueta = result["cr"] or f"{len(result['commits'])} commit(s)"
    print(f"LANE-INTEGRITY {etiqueta} — {result['veredicto']}", file=stream)
    print(f"revisión auditada: {result['revision'][:12]} · "
          f"commits: {len(result['commits'])}", file=stream)
    print("", file=stream)
    print("| check | qué mide | resultado |", file=stream)
    print("|---|---|---|", file=stream)
    etiquetas = {
        "a": "gate e implementación en el mismo commit",
        "b": "gate anterior al primer commit de implementación",
        "c": "status ≤ candidato salvo QA independiente",
        "d": "evidencia citada existe (y sha256 coincide)",
        "e": "0 push / 0 tag en el rango",
    }
    for key, valor in result["matriz"].items():
        print(f"| ({key}) | {etiquetas[key]} | {valor} |", file=stream)
    print("", file=stream)
    if result["hallazgos"]:
        print("hallazgos:", file=stream)
        for h in result["hallazgos"]:
            print(f"  {h['severity']:4s} [{h['code']}] {h['detail']}", file=stream)
    else:
        print("hallazgos: ninguno", file=stream)
    print("", file=stream)
    print(f"RESUMEN: {result['fallos']} FAIL · {result['avisos']} WARN · "
          f"exit {EXIT_PASS if result['veredicto'] == 'PASS' else EXIT_FAIL}", file=stream)


# ── selftest (cadenas de commits colgantes SOBRE ESTE REPO) ──────────────────

SELFTEST_CR = "CR-selftest-51d25f1e"
SELFTEST_GATE = "scripts/zz-selftest-gate-001.py"
SELFTEST_IMPL = "src/zz-selftest-impl.rs"
SELFTEST_HH1 = "docs/sintergic/handoffs/zz-selftest-hh1.md"
SELFTEST_INDEX = DEFAULT_INDEX


def _hash_blob(repo: Path, data: bytes) -> str:
    rc, out, err = git(repo, "hash-object", "-w", "--stdin", stdin=data)
    if rc != 0:
        raise RuntimeError(f"hash-object falló: {err.strip()}")
    return out.decode().strip()


def _build_commit(repo: Path, base: str, files: dict[str, bytes],
                  message: str) -> str:
    """Commit colgante sobre `base` con `files` escritos. NO mueve HEAD, ni refs,
    ni el índice del repo: usa un GIT_INDEX_FILE temporal y `commit-tree`."""
    fd, idx_path = tempfile.mkstemp(prefix="lane-integrity-idx.")
    os.close(fd)
    os.unlink(idx_path)
    env = {"GIT_INDEX_FILE": idx_path,
           "GIT_AUTHOR_NAME": "lane-integrity-selftest",
           "GIT_AUTHOR_EMAIL": "selftest@example.invalid",
           "GIT_COMMITTER_NAME": "lane-integrity-selftest",
           "GIT_COMMITTER_EMAIL": "selftest@example.invalid"}
    try:
        rc, _, err = git(repo, "read-tree", base, env=env)
        if rc != 0:
            raise RuntimeError(f"read-tree falló: {err.strip()}")
        for path, data in files.items():
            blob = _hash_blob(repo, data)
            rc, _, err = git(repo, "update-index", "--add", "--cacheinfo",
                             f"100644,{blob},{path}", env=env)
            if rc != 0:
                raise RuntimeError(f"update-index {path} falló: {err.strip()}")
        rc, out, err = git(repo, "write-tree", env=env)
        if rc != 0:
            raise RuntimeError(f"write-tree falló: {err.strip()}")
        tree = out.decode().strip()
        rc, out, err = git(repo, "commit-tree", tree, "-p", base, "-m", message, env=env)
        if rc != 0:
            raise RuntimeError(f"commit-tree falló: {err.strip()}")
        return out.decode().strip()
    finally:
        try:
            os.unlink(idx_path)
        except OSError:
            pass


def _index_append(repo: Path, base: str, row: dict, index_path: str) -> bytes:
    current = bytes_at(repo, base, index_path) or b""
    if current and not current.endswith(b"\n"):
        current += b"\n"
    return current + (json.dumps(row, ensure_ascii=False) + "\n").encode()


def _gate_existente(repo: Path, base: str) -> str:
    """Primer gate presente en el árbol de `base` (para editar uno real en el caso
    (i)). Lista el ÍNDICE del repo (plumbing) y confirma existencia en `base`."""
    preferido = "scripts/shell-001.py"
    if exists_at(repo, base, preferido):
        return preferido
    for path in git_text(repo, "ls-files", "scripts").splitlines():
        path = path.strip()
        if path and RE_GATE.match(path) and exists_at(repo, base, path):
            return path
    return ""


def _hh1_bytes(status: str, cited: list[str], author: str = "selftest-ejecutor") -> bytes:
    citas = "\n".join(f"- `{c}`" for c in cited)
    return (
        "---\n"
        "schema: HH-1\n"
        f"id: HH-selftest-{status.lower()}\n"
        f"cr: {SELFTEST_CR}\n"
        "date: 2026-10-29\n"
        f"agent: {author}\n"
        "status: " + status + "\n"
        "escalation: none\n"
        "---\n"
        "<!-- CEP: selftest de lane-integrity; este HH-1 vive en un commit colgante. -->\n"
        "## Hecho\n"
        f"{citas}\n"
    ).encode()


def _selftest_cases(repo: Path, base: str
                    ) -> tuple[list[tuple[str, str, list[str], str, str | None, list[str]]],
                               list[str]]:
    """Devuelve ([(id, esperado, códigos-FAIL-esperados, tip, cr, códigos-WARN-
    esperados)], [rutas-de-limpieza]) construyendo las cadenas. Cada caso es
    independiente (su propia cadena desde `base`).

    Dos controles extra (CR-9dd5c91e) blindan dos falsos positivos por diseño del
    propio instrumento, medidos sobre lanes ya cerradas de este target:
      · `corto`     — el CR llega como id corto sin prefijo (`selftest`): si `cr_match`
                      no lo resuelve, el caso sale SIN_COMMITS/FAIL (el defecto real).
      · `artefacto` — una cita que es DIRECTORIO presente en disco y no versionada:
                      debe salir WARN EVIDENCIA_NO_VERSIONADA, nunca FAIL.
    """
    casos = []

    # (i) commit que EDITA un gate existente + impl a la vez. Si hay un gate en el
    # árbol de `base` se edita ése (así (b) queda satisfecha y el único hallazgo es
    # A-MIXTO: el caso mide lo que dice medir); si no lo hay, el gate nace en el
    # mismo commit y el caso añade GATE_POSTERIOR, que es verdad y no se esconde.
    gate_previo = _gate_existente(repo, base)
    msg = f"selftest mixto {SELFTEST_CR}"
    if gate_previo:
        actual = bytes_at(repo, base, gate_previo) or b""
        files_i = {gate_previo: actual + b"# editado por el selftest\n",
                   SELFTEST_IMPL: b"// impl\n"}
    else:
        files_i = {SELFTEST_GATE: b"# gate\n", SELFTEST_IMPL: b"// impl\n"}
    # el caso lleva su HH-1 `candidato` dentro de la MISMA cadena: si faltara, (c)
    # añadiría HH1_AUSENTE y el caso dejaría de medir sólo lo que dice medir
    hh1_1 = _hh1_bytes("candidato", [gate_previo or SELFTEST_GATE])
    files_i[SELFTEST_HH1] = hh1_1
    files_i[SELFTEST_INDEX] = _index_append(repo, base, {
        "schema": "HH-1", "id": "HH-selftest-mixto", "cr": SELFTEST_CR,
        "date": "2026-10-29", "agent": "selftest-ejecutor", "status": "candidato",
        "escalation": "none", "path": SELFTEST_HH1,
        "sha256": hashlib.sha256(hh1_1).hexdigest(),
    }, SELFTEST_INDEX)
    m = _build_commit(repo, base, files_i, msg)
    casos.append(("mixto", "FAIL", ["A-MIXTO"], m, None, []))

    # (ii) gate POSTERIOR a la implementación (con su HH-1 en la cadena, igual que (i))
    hh1_2 = _hh1_bytes("candidato", [SELFTEST_IMPL])
    i0 = _build_commit(repo, base, {
        SELFTEST_IMPL: b"// impl sin gate previo\n",
        SELFTEST_HH1: hh1_2,
        SELFTEST_INDEX: _index_append(repo, base, {
            "schema": "HH-1", "id": "HH-selftest-posterior", "cr": SELFTEST_CR,
            "date": "2026-10-29", "agent": "selftest-ejecutor", "status": "candidato",
            "escalation": "none", "path": SELFTEST_HH1,
            "sha256": hashlib.sha256(hh1_2).hexdigest(),
        }, SELFTEST_INDEX),
    }, f"selftest impl sin gate {SELFTEST_CR}")
    gpost = _build_commit(repo, i0, {SELFTEST_GATE: b"# gate que nace tarde\n"},
                          f"selftest gate posterior {SELFTEST_CR}")
    casos.append(("posterior", "FAIL", ["GATE_POSTERIOR"], gpost, None, []))

    # (iii) evidencia citada con path inexistente
    g3 = _build_commit(repo, base, {SELFTEST_GATE: b"# gate\n"},
                       f"selftest gate para evidencia {SELFTEST_CR}")
    hh1_3 = _hh1_bytes("candidato", ["docs/selftest-no-existe/evidencia.md"])
    idx3 = _index_append(repo, g3, {
        "schema": "HH-1", "id": "HH-selftest-evidencia", "cr": SELFTEST_CR,
        "date": "2026-10-29", "agent": "selftest-ejecutor", "status": "candidato",
        "escalation": "none", "path": SELFTEST_HH1,
        "sha256": hashlib.sha256(hh1_3).hexdigest(),
    }, SELFTEST_INDEX)
    e3 = _build_commit(repo, g3, {SELFTEST_HH1: hh1_3, SELFTEST_INDEX: idx3},
                       f"selftest evidencia citada {SELFTEST_CR}")
    casos.append(("evidencia", "FAIL", ["EVIDENCIA_AUSENTE"], e3, None, []))

    # (iv) status DONE sin fila de QA independiente
    g4 = _build_commit(repo, base, {SELFTEST_GATE: b"# gate\n"},
                       f"selftest gate para status {SELFTEST_CR}")
    hh1_4 = _hh1_bytes("DONE", [SELFTEST_GATE])
    idx4 = _index_append(repo, g4, {
        "schema": "HH-1", "id": "HH-selftest-status", "cr": SELFTEST_CR,
        "date": "2026-10-29", "agent": "selftest-ejecutor", "status": "DONE",
        "escalation": "none", "path": SELFTEST_HH1,
        "sha256": hashlib.sha256(hh1_4).hexdigest(),
    }, SELFTEST_INDEX)
    s4 = _build_commit(repo, g4, {SELFTEST_HH1: hh1_4, SELFTEST_INDEX: idx4},
                       f"selftest status alto {SELFTEST_CR}")
    casos.append(("status", "FAIL", ["STATUS_SIN_QA"], s4, None, []))

    # (v) control de NO falso rojo: gate antes, impl después, HH-1 candidato
    g5 = _build_commit(repo, base, {SELFTEST_GATE: b"# gate\n"},
                       f"selftest pase gate {SELFTEST_CR}")
    i5 = _build_commit(repo, g5, {SELFTEST_IMPL: b"// impl contra gate previo\n"},
                       f"selftest pase impl {SELFTEST_CR}")
    hh1_5 = _hh1_bytes("candidato", [SELFTEST_GATE])
    idx5 = _index_append(repo, i5, {
        "schema": "HH-1", "id": "HH-selftest-pase", "cr": SELFTEST_CR,
        "date": "2026-10-29", "agent": "selftest-ejecutor", "status": "candidato",
        "escalation": "none", "path": SELFTEST_HH1,
        "sha256": hashlib.sha256(hh1_5).hexdigest(),
    }, SELFTEST_INDEX)
    p5 = _build_commit(repo, i5, {SELFTEST_HH1: hh1_5, SELFTEST_INDEX: idx5},
                       f"selftest pase cierre {SELFTEST_CR}")
    casos.append(("pase", "PASS", [], p5, None, []))

    # (vi) control de falso positivo CERRADO: cita que es DIRECTORIO en disco y no
    # versionada. La cadena de commits es idéntica a la de `pase` (gate antes, impl
    # después, HH-1 candidato): lo único que cambia es la cita del HH-1, así que si
    # el caso no mide lo que dice medir, sale FALLA por otro código y se ve.
    dir_ev_rel = ".sintergic/artifacts/zz-selftest-dir"
    dir_ev = repo / dir_ev_rel
    dir_ev.mkdir(parents=True, exist_ok=True)
    if not dir_ev.is_dir():
        raise RuntimeError(f"selftest: no se pudo materializar {dir_ev_rel} (caso vacuo)")
    limpieza = [str(dir_ev)]
    hh1_6 = _hh1_bytes("candidato", [dir_ev_rel + "/"])
    idx6 = _index_append(repo, i5, {
        "schema": "HH-1", "id": "HH-selftest-candidato", "cr": SELFTEST_CR,
        "date": "2026-10-29", "agent": "selftest-ejecutor", "status": "candidato",
        "escalation": "none", "path": SELFTEST_HH1,
        "sha256": hashlib.sha256(hh1_6).hexdigest(),
    }, SELFTEST_INDEX)
    a6 = _build_commit(repo, i5, {SELFTEST_HH1: hh1_6, SELFTEST_INDEX: idx6},
                       f"selftest artefacto directorio {SELFTEST_CR}")
    casos.append(("artefacto", "PASS", [], a6, None, ["EVIDENCIA_NO_VERSIONADA"]))

    # (vii) control de resolución: el MISMO cierre de `pase`, auditado con el CR en
    # forma corta sin prefijo. Debe dar el MISMO veredicto (PASS): lo que se mide es
    # `cr_match`, no el cierre.
    casos.append(("corto", "PASS", [], p5, "selftest", []))

    return casos, limpieza


def run_selftest(repo: Path, only: str | None, as_json: bool) -> int:
    base = head_sha(repo)
    if not base:
        print("selftest: el repo no tiene commits (HEAD irresoluble)", file=sys.stderr)
        return EXIT_USO
    # El CR sintético se aísla por token: los mensajes de las cadenas colgantes lo
    # llevan, y ningún commit real lo lleva (se comprueba, si no el caso es vacuO).
    reales = resolve_cr_commits(repo, SELFTEST_CR, base)
    if reales:
        print(f"selftest ABORTADO: {len(reales)} commit(s) reales ya llevan {SELFTEST_CR}; "
              f"el CR de selftest debe ser único", file=sys.stderr)
        return EXIT_USO

    casos, limpieza = _selftest_cases(repo, base)
    if only:
        casos = [c for c in casos if c[0] == only]
        if not casos:
            print(f"selftest: caso desconocido `{only}`", file=sys.stderr)
            return EXIT_USO

    filas, fallos, detalles = [], 0, {}
    for caso_id, esperado, codigos, tip, cr_caso, warns in casos:
        # (iii)/(iv)/(v) resuelven por CR y leen INDEX/HH-1 en el tip de la cadena;
        # (i)/(ii) auditan el rango base..tip.
        cr_use = cr_caso or SELFTEST_CR
        resultado = run_audit(
            repo, cr_use, resolve_cr_commits(repo, cr_use, tip), tip,
            SELFTEST_INDEX, DEFAULT_HANDOFFS, DEFAULT_VALIDATION)
        obtenido = resultado["veredicto"]
        hallados = [h["code"] for h in resultado["hallazgos"] if h["severity"] == "FAIL"]
        todos = [h["code"] for h in resultado["hallazgos"]]
        detalles[caso_id] = resultado
        faltan = [c for c in codigos if c not in hallados]
        faltan_warn = [c for c in warns if c not in todos]
        ok = obtenido == esperado and not faltan and not faltan_warn
        if not ok:
            fallos += 1
        filas.append({
            "caso": caso_id, "esperado": esperado, "obtenido": obtenido,
            "codigos_esperados": codigos,
            "codigos_observados": hallados,
            "warns_esperados": warns,
            "warns_observados": [c for c in todos if c in warns and c not in hallados],
            "commits": len(resultado["commits"]), "tip": tip[:12],
            "veredicto": "OK" if ok else "FALLA",
        })

    for ruta_limpieza in limpieza:
        try:
            os.rmdir(ruta_limpieza)
        except OSError:
            pass

    if as_json:
        print(json.dumps({"base": base, "filas": filas, "fallos": fallos}, indent=2))
    else:
        print(f"SELFTEST lane-integrity — base {base[:12]} · cadenas colgantes sobre ESTE repo")
        print("| caso | esperado | obtenido | FAIL esperados | FAIL observados | "
              "WARN exigidos | WARN observados | tip | veredicto |")
        print("|---|---|---|---|---|---|---|---|---|")
        for fila in filas:
            print(f"| {fila['caso']} | {fila['esperado']} | {fila['obtenido']} | "
                  f"{','.join(fila['codigos_esperados']) or '-'} | "
                  f"{','.join(fila['codigos_observados']) or '-'} | "
                  f"{','.join(fila['warns_esperados']) or '-'} | "
                  f"{','.join(fila['warns_observados']) or '-'} | {fila['tip']} | "
                  f"{fila['veredicto']} |")
        print("")
        print(f"RESUMEN: {len(filas) - fallos}/{len(filas)} casos OK · exit "
              f"{EXIT_PASS if fallos == 0 else EXIT_FAIL}")
        if only and only in detalles:
            # el caso aislado imprime TAMBIÉN el informe real del instrumento: es la
            # prueba que se pega en el HH-1 (no un resumen de la expectativa).
            print("")
            print(f"— informe real del instrumento para el caso `{only}` —")
            print("")
            print(f"(cadena colgante sobre {base[:12]}; tip {detalles[only]['revision'][:12]})")
            print("")
            render(detalles[only])
    return EXIT_PASS if fallos == 0 else EXIT_FAIL


# ── main ─────────────────────────────────────────────────────────────────────

def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(
        prog="lane-integrity.py",
        description="Auditor de cierre de lane: PASS/FAIL sobre git + HH-1 + INDEX.")
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--cr", help="CR de la lane (corto o completo)")
    group.add_argument("--commits", help="rango git `a..b`")
    group.add_argument("--selftest", action="store_true",
                       help="7 casos (4 negativos + 3 controles) sobre ESTE repo")
    parser.add_argument("--at", help="revisión donde leer HH-1/INDEX (default HEAD)")
    parser.add_argument("--repo", default=".", help="raíz del repo (default cwd)")
    parser.add_argument("--index", default=DEFAULT_INDEX)
    parser.add_argument("--handoffs-dir", default=DEFAULT_HANDOFFS)
    parser.add_argument("--validation-dir", default=DEFAULT_VALIDATION)
    parser.add_argument("--hh1", help="forzar un HH-1 concreto (ruta relativa)")
    parser.add_argument("--case", help="con --selftest: ejecutar un solo caso")
    parser.add_argument("--json", action="store_true", help="salida JSON")
    args = parser.parse_args(argv)

    repo = Path(args.repo).resolve()
    rc_git, _, _ = git(repo, "rev-parse", "--git-dir")
    if rc_git != 0:
        print(f"lane-integrity: {repo} no es un repo git", file=sys.stderr)
        return EXIT_USO

    if args.selftest:
        return run_selftest(repo, args.case, args.json)
    if args.case:
        print("lane-integrity: --case solo vale con --selftest", file=sys.stderr)
        return EXIT_USO

    at_rev = rev_parse(repo, args.at or "HEAD")
    if not at_rev:
        print(f"lane-integrity: revisión `{args.at or 'HEAD'}` irresoluble", file=sys.stderr)
        return EXIT_USO

    try:
        if args.cr:
            commits = resolve_cr_commits(repo, args.cr, at_rev)
            cr = args.cr
        else:
            _, commits, tip = resolve_range(repo, args.commits)
            if not args.at:
                at_rev = tip
            cr = None
    except ValueError as exc:
        print(f"lane-integrity: {exc}", file=sys.stderr)
        return EXIT_USO

    resultado = run_audit(repo, cr, commits, at_rev, args.index,
                          args.handoffs_dir, args.validation_dir, args.hh1)
    if args.json:
        print(json.dumps(resultado, indent=2, ensure_ascii=False))
    else:
        render(resultado)
    return EXIT_PASS if resultado["veredicto"] == "PASS" else EXIT_FAIL


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
