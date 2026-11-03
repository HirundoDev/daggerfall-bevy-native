#!/usr/bin/env python3
"""derive-snd-goldens.py - derive and verify the DAGGER.SND clip goldens.

The goldens are MEASURED from the retail bank, never tuned to the code. The DFU
catalogue (``reference/dfu-spec/Assets/Scripts/SoundClips.cs``, MIT, pinned)
supplies the member names and discriminants; the discriminant is the positional
directory index DFU plays (``SoundReader.cs:59-101`` -> ``SndFile.GetSound`` ->
``bsaFile.GetRecordProxy(sound)``), never the entry's ``RecordID``. The retail
``ARENA2/DAGGER.SND`` supplies the payload of the entry at that position.

Two modes, one derivation:

  generate (writes the TSV):
    python3 scripts/audio/derive-snd-goldens.py \
        --dfu reference/dfu-spec/Assets/Scripts/SoundClips.cs \
        --snd "$ARENA2/DAGGER.SND" \
        --out crates/daggerfall_parser_rs/goldens/sound-clips-375.tsv

  verify (exit 0 clean, exit 1 with a per-row diff):
    python3 scripts/audio/derive-snd-goldens.py \
        --snd "$ARENA2/DAGGER.SND" \
        --tsv crates/daggerfall_parser_rs/goldens/sound-clips-375.tsv

Container layout read from the retail file (BSA type 0x0200): u16 record count,
u16 directory type, payloads packed from offset 4 in directory order, trailing
directory of ``<u32 RecordID, u32 DataSize>`` records. The invariant
``4 + sum(DataSize) == directory_offset`` is enforced. The retail directory is
NOT sorted by RecordID (the first ids are 3, 4, 5, 6, 7, 8, 375, 9), so the
position is the only valid key.

Verification re-derives every payload column from the bank and compares it with
the committed file; it never calls Rust, so the producer and the consumer of the
goldens stay independent. Stdlib only.
"""

import argparse
import hashlib
import re
import struct
import sys
from pathlib import Path

# BSA directory type with numeric IDs (dfbsa.txt): <u32 RecordID, u32 DataSize>.
BSA_TYPE_NUMBERED = 0x0200
# Numbered directory record size in bytes.
NUMBERED_RECORD_SIZE = 8
# Payloads start right after the 4-byte header.
DATA_START = 4
# DFU SoundClips members expected in the catalogue (duplicates included).
EXPECTED_MEMBERS = 375
# FNV-1a 64 parameters (the digest published in the golden file).
FNV_BASIS = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3
# Columns of a golden row, in order.
COLUMNS = ("name", "discriminant", "expect", "record_id", "samples",
           "fnv1a64", "first", "last")
NA = "-"

ENUM_RX = re.compile(r"public\s+enum\s+SoundClips\s*\{(.*?)\n\s*\}", re.S)
MEMBER_RX = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(-?\d+)\s*,?$")


class DeriveError(Exception):
    """Input that cannot be measured (wrong container, missing member, bad file)."""


class Row:
    """One measured catalogue row."""

    __slots__ = ("name", "discriminant", "expect", "record_id", "samples",
                 "fnv1a64", "first", "last")

    def __init__(self, name, discriminant, expect, record_id, samples,
                 fnv1a64, first, last):
        self.name = name
        self.discriminant = discriminant
        self.expect = expect
        self.record_id = record_id
        self.samples = samples
        self.fnv1a64 = fnv1a64
        self.first = first
        self.last = last

    def measured(self):
        """The measured columns (everything but name/discriminant)."""
        return (self.expect, self.record_id, self.samples, self.fnv1a64,
                self.first, self.last)


def fnv1a64(payload):
    """FNV-1a 64 over the raw payload bytes."""
    digest = FNV_BASIS
    for byte in payload:
        digest ^= byte
        digest = (digest * FNV_PRIME) & 0xFFFFFFFFFFFFFFFF
    return digest


def strip_csharp_comments(text):
    """Removes ``//`` line comments and ``/* */`` blocks (an enum holds no strings)."""
    out = []
    pos = 0
    while pos < len(text):
        if text.startswith("/*", pos):
            end = text.find("*/", pos + 2)
            if end < 0:
                raise DeriveError("unterminated /* */ comment")
            out.append(" ")
            pos = end + 2
        elif text.startswith("//", pos):
            end = text.find("\n", pos)
            pos = len(text) if end < 0 else end
        else:
            out.append(text[pos])
            pos += 1
    return "".join(out)


def parse_dfu_catalog(path):
    """Parses the DFU SoundClips enum: ``[(name, discriminant)]`` in source order.

    Comments are stripped first, so a commented-out member never enters the
    catalogue; preprocessor lines (``#region``/``#endregion``) are skipped.
    Duplicate names are rejected, duplicate values are kept (the alias).
    """
    text = strip_csharp_comments(Path(path).read_text(encoding="utf-8"))
    match = ENUM_RX.search(text)
    if match is None:
        raise DeriveError("SoundClips enum body not found in %s" % path)
    members = []
    for line in match.group(1).splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        entry = MEMBER_RX.match(stripped)
        if entry is None:
            raise DeriveError("unparsed SoundClips line: %r" % stripped)
        members.append((entry.group(1), int(entry.group(2))))
    names = [name for name, _ in members]
    if len(set(names)) != len(names):
        raise DeriveError("duplicate SoundClips member name")
    if len(members) != EXPECTED_MEMBERS:
        raise DeriveError("SoundClips members: expected %d, found %d"
                          % (EXPECTED_MEMBERS, len(members)))
    return members


def read_bank(path):
    """Reads a BSA 0x0200 bank: ``(sha256, record_count, bsa_type, entries, payloads)``."""
    data = Path(path).read_bytes()
    if len(data) < DATA_START:
        raise DeriveError("%s: %d bytes is shorter than the BSA header"
                          % (path, len(data)))
    record_count, bsa_type = struct.unpack_from("<HH", data, 0)
    if bsa_type != BSA_TYPE_NUMBERED:
        raise DeriveError("%s: BSA type 0x%04x is not 0x0200"
                          % (path, bsa_type))
    directory_offset = len(data) - record_count * NUMBERED_RECORD_SIZE
    if directory_offset < DATA_START:
        raise DeriveError("%s: directory offset %d is inside the header"
                          % (path, directory_offset))
    entries = []
    computed_end = DATA_START
    for index in range(record_count):
        record_id, data_size = struct.unpack_from(
            "<II", data, directory_offset + index * NUMBERED_RECORD_SIZE)
        entries.append((record_id, data_size))
        computed_end += data_size
    if computed_end != directory_offset:
        raise DeriveError(
            "%s: layout mismatch, payloads end at %d but the directory starts "
            "at %d" % (path, computed_end, directory_offset))
    payloads = []
    offset = DATA_START
    for _, data_size in entries:
        payloads.append(data[offset:offset + data_size])
        offset += data_size
    return hashlib.sha256(data).hexdigest(), record_count, bsa_type, entries, payloads


def derive_rows(members, record_count, entries, payloads):
    """Measures one row per DFU member, in source order."""
    rows = []
    for name, discriminant in members:
        if discriminant < 0:
            rows.append(Row(name, discriminant, "none", None, 0, None, None, None))
            continue
        if discriminant >= record_count:
            raise DeriveError(
                "%s = %d has no directory entry (the bank has %d records)"
                % (name, discriminant, record_count))
        record_id, _ = entries[discriminant]
        payload = payloads[discriminant]
        if not payload:
            rows.append(Row(name, discriminant, "empty", record_id, 0,
                            None, None, None))
        else:
            rows.append(Row(name, discriminant, "pcm", record_id, len(payload),
                            fnv1a64(payload), payload[0], payload[-1]))
    return rows


def format_cell(value, hex_width=0):
    """Renders ``-`` for a non-applicable column."""
    if value is None:
        return NA
    if hex_width:
        return "%0*x" % (hex_width, value)
    return str(value)


def format_row(row):
    """Renders one golden row (8 tab-separated columns)."""
    return "\t".join((
        row.name,
        str(row.discriminant),
        row.expect,
        format_cell(row.record_id),
        str(row.samples),
        format_cell(row.fnv1a64, 16),
        format_cell(row.first),
        format_cell(row.last),
    ))


def render(rows, snd_sha256, record_count, bsa_type):
    """Renders the complete golden file (header records + one line per member)."""
    lines = [
        "# DAGGER.SND goldens \u2014 measured from retail; see scripts/audio/derive-snd-goldens.py",
        "# snd_sha256=%s \u00b7 records=%d \u00b7 bsa=0x%04x \u00b7 fnv1a64(basis %#x, prime %#x)"
        % (snd_sha256, record_count, bsa_type, FNV_BASIS, FNV_PRIME),
        "# " + "\t".join(COLUMNS),
    ]
    lines.extend(format_row(row) for row in rows)
    return "\n".join(lines) + "\n"


def split_row(line):
    """Splits a golden row, or raises ``DeriveError`` with the exact problem."""
    cells = line.split("\t")
    if len(cells) != len(COLUMNS):
        raise DeriveError("expected %d columns, found %d" % (len(COLUMNS), len(cells)))
    return cells


def parse_golden_text(text):
    """Parses a golden file into ``(header_snd_sha256, rows)``."""
    snd_sha256 = None
    rows = []
    for line_no, line in enumerate(text.splitlines(), start=1):
        if line.startswith("#"):
            marker = "snd_sha256="
            if marker in line:
                value = line.split(marker, 1)[1].split(" ", 1)[0].strip()
                if value:
                    snd_sha256 = value
            continue
        cells = split_row(line)
        try:
            discriminant = int(cells[1])
            samples = int(cells[4])
        except ValueError as error:
            raise DeriveError("line %d: %s" % (line_no, error))
        if samples < 0:
            raise DeriveError("line %d: negative sample count %d" % (line_no, samples))
        record_id = None if cells[3] == NA else int(cells[3])
        fnv = None if cells[5] == NA else int(cells[5], 16)
        first = None if cells[6] == NA else int(cells[6])
        last = None if cells[7] == NA else int(cells[7])
        rows.append(Row(cells[0], discriminant, cells[2], record_id, samples,
                        fnv, first, last))
    return snd_sha256, rows


def verify_tsv(snd_path, tsv_path):
    """Re-derives every measured column and returns the list of diffs."""
    snd_sha256, record_count, _, entries, payloads = read_bank(snd_path)
    text = Path(tsv_path).read_text(encoding="utf-8")
    header_sha, rows = parse_golden_text(text)
    diffs = []
    if header_sha is None:
        diffs.append("header: missing snd_sha256 record")
    elif header_sha != snd_sha256:
        diffs.append("header: snd_sha256=%s but the bank is %s"
                     % (header_sha, snd_sha256))
    if len(rows) != EXPECTED_MEMBERS:
        diffs.append("header: expected %d rows, found %d"
                     % (EXPECTED_MEMBERS, len(rows)))
    names = set()
    for index, row in enumerate(rows):
        if row.name in names:
            diffs.append("row %d %s: duplicate name" % (index + 1, row.name))
        names.add(row.name)
        if row.discriminant < 0:
            measured = Row(row.name, row.discriminant, "none", None, 0,
                           None, None, None)
        elif row.discriminant >= record_count:
            diffs.append("row %d %s: discriminant %d has no directory entry "
                         "(the bank has %d records)"
                         % (index + 1, row.name, row.discriminant, record_count))
            continue
        else:
            measured = derive_rows(
                [(row.name, row.discriminant)], record_count, entries, payloads)[0]
        if row.measured() != measured.measured():
            for column, got, want in zip(COLUMNS[2:], row.measured(),
                                         measured.measured()):
                if got != want:
                    diffs.append("row %d %s: %s tsv=%s derived=%s"
                                 % (index + 1, row.name, column, got, want))
    return diffs, record_count, snd_sha256


def counts(rows):
    """Per-expect counts, in ``pcm/empty/none`` order."""
    return {expect: sum(1 for row in rows if row.expect == expect)
            for expect in ("pcm", "empty", "none")}


def generate(dfu_path, snd_path, out_path):
    """Measures the catalogue and writes the TSV; returns the per-expect counts."""
    members = parse_dfu_catalog(dfu_path)
    snd_sha256, record_count, bsa_type, entries, payloads = read_bank(snd_path)
    rows = derive_rows(members, record_count, entries, payloads)
    Path(out_path).write_text(render(rows, snd_sha256, record_count, bsa_type),
                              encoding="utf-8")
    return rows


def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Derive (or verify) the DAGGER.SND clip goldens from retail.")
    parser.add_argument("--dfu", help="DFU SoundClips.cs (generate mode)")
    parser.add_argument("--snd", required=True, help="retail DAGGER.SND")
    parser.add_argument("--out", help="golden TSV to write (generate mode)")
    parser.add_argument("--tsv", help="golden TSV to check (verify mode)")
    args = parser.parse_args(argv)

    if args.out and args.tsv:
        parser.error("--out and --tsv are mutually exclusive")
    try:
        if args.out:
            if not args.dfu:
                parser.error("--out requires --dfu")
            rows = generate(args.dfu, args.snd, args.out)
            tally = counts(rows)
            print("generate: %d rows (pcm=%d empty=%d none=%d) -> %s"
                  % (len(rows), tally["pcm"], tally["empty"], tally["none"],
                     args.out))
            return 0
        if args.tsv:
            diffs, record_count, snd_sha256 = verify_tsv(args.snd, args.tsv)
            if diffs:
                for diff in diffs:
                    print("DIFF %s" % diff)
                print("verify: FAIL (%d differences) - %s"
                      % (len(diffs), args.tsv))
                return 1
            print("verify: OK (%d rows) - %s matches the bank %s (%d records)"
                  % (EXPECTED_MEMBERS, args.tsv, snd_sha256[:16], record_count))
            return 0
        if args.dfu:
            parser.error("--dfu alone derives nothing: pass --out to generate")
        parser.error("pass --out (generate) or --tsv (verify)")
    except DeriveError as error:
        print("error: %s" % error, file=sys.stderr)
        return 2
    except OSError as error:
        print("error: %s" % error, file=sys.stderr)
        return 2
    return 2


if __name__ == "__main__":
    sys.exit(main())
