#!/usr/bin/env python3
"""fg01-door-census.py — retail door census from RAW BYTES (FG-01 `GAP-FG01-02`).

An **independent** oracle of the Rust parser: reads BSA/DOS, `.RMB` and `ARCH3D` with
pure stdlib (0 dependencies, 0 `daggerfall_parser_rs`), so that its figures
can contradict the engine instead of repeating it (a tautology measures nothing).

What it counts
----------
1. **`.RMB`** (`BLOCKS.BSA`): block sub-records and their two `BlockData`
      (exterior + interior), each with its door list (`door_count` = byte
      +4 of the 17 B header). Layout residue per sub-record: declared bytes
      (`BlockDataSizes[i]`) − bytes consumed by the two `BlockData`.
2. **`ARCH3D.BSA`**: planes whose base texture archive is **74**
      (`DFU:MeshReader.cs:614` `BuildingDoors = 74`; `:645` reduction
      `archive -= archive/100*100` only if `archive > 100` and it is not 331/156). Those
      are the facade doors the building model embeds: the retail `.RMB`
      does not carry them (0 in the exterior of the 9005 sub-records).
3. **What the world composes**: `MAPS.BSA` `MAPPITEM.<region>` + `MAPNAMES` →
      cities (capital + small ones ≤ 9 cells, same deterministic selection as
      `PROBE-RMB-001`) → `.RMB` names per cell (`DFU:MapsFile.cs:855-893`) →
      composed sub-records and 74 planes of the exterior models those
      buildings instantiate.
4. **What reaches the scene today**: read from the app's own instrument
      (`FG01-02 VE: world doors composed=<n> retail=<n> scenario=<n>`). Without
      `--world` that datum is UNVERIFIED (never estimated).

Output
------
Normalized rows `row <name>=<value>` (no timestamps, no PIDs) + detail in
`--out DIR` (default `.sintergic/artifacts/fg01-02/`, gitignored) + `--json`.

Exit: `0` full census · `1` some UNVERIFIED datum (declared in
`row unverified=`) · `2` usage/IO.

Usage:
  scripts/fg01-door-census.py [--data-root DIR] [--out DIR] [--json]
                              [--world LOG]
"""
import argparse
import hashlib
import json
import os
import struct
import sys

WS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_DATA_ROOT = os.path.expanduser("~/Games/Daggerfall/app")
DEFAULT_OUT = os.path.join(WS, ".sintergic", "artifacts", "fg01-02")

REGION = "017"
CAPITAL_CITY = "Daggerfall"
CITIES_N = 5
SMALL_CITY_MAX_BLOCKS = 9

# `rmb.rs:44` — cabecera fija del Map Block Record.
RMB_HEADER_LEN = 6776
MAX_SUB_RECORDS = 32
# `rmb.rs:309` — `BlockDataSizes[32]` empieza en 1475+128.
BLOCK_DATA_SIZES_AT = 1475 + 128
# `rmb.rs` parse_block_data: cabecera 17 B (5 contadores u8 + 3×i32) y records.
BLOCK_DATA_HEADER = 17
MODEL_LEN = 66
FLAT_LEN = 17
SECTION3_LEN = 16
PERSON_LEN = 17
DOOR_LEN = 19

#  `MeshReader.cs:614` — texture archive of the building doors.
BUILDING_DOORS_ARCHIVE = 74
#  `maps.rs:48-56` — prefix table per BlockIndex and second letter.
RMB_BLOCK_PREFIXES = [
    "TVRN", "GENR", "RESI", "WEAP", "ARMR", "ALCH", "BANK", "BOOK", "CLOT", "FURN",
    "GEMS", "LIBR", "PAWN", "TEMP", "TEMP", "PALA", "FARM", "DUNG", "CAST", "MANR",
    "SHRI", "RUIN", "SHCK", "GRVE", "FILL", "KRAV", "KDRA", "KOWL", "KMOO", "KCAN",
    "KFLA", "KHOR", "KROS", "KWHE", "KSCA", "KHAW", "MAGE", "THIE", "DARK", "FIGH",
    "CUST", "WALL", "MARK", "SHIP", "WITC",
]
LETTER2_ARRAY = ["A", "L", "M", "S"]
# `maps.rs` LOCATION_NAME_LEN (LocationRecordElementHeader `Name[32]`).
LOCATION_NAME_LEN = 32


class CensusError(Exception):
    """Dato ilegible: se declara NO-VERIFICADO, nunca se rellena."""


#  ── little-endian primitives with bounds checking ─────────────────────────
def u8(b, at):
    if at + 1 > len(b):
        raise CensusError("u8 out of bounds at %d (len %d)" % (at, len(b)))
    return b[at]


def u16(b, at):
    if at + 2 > len(b):
        raise CensusError("u16 out of bounds at %d (len %d)" % (at, len(b)))
    return struct.unpack_from("<H", b, at)[0]


def i16(b, at):
    if at + 2 > len(b):
        raise CensusError("i16 out of bounds at %d (len %d)" % (at, len(b)))
    return struct.unpack_from("<h", b, at)[0]


def u32(b, at):
    if at + 4 > len(b):
        raise CensusError("u32 out of bounds at %d (len %d)" % (at, len(b)))
    return struct.unpack_from("<I", b, at)[0]


def i32(b, at):
    if at + 4 > len(b):
        raise CensusError("i32 out of bounds at %d (len %d)" % (at, len(b)))
    return struct.unpack_from("<i", b, at)[0]


def c_string32(raw):
    end = raw.find(b"\0")
    if end < 0:
        end = len(raw)
    return raw[:end].decode("latin-1", "replace")


# ── BSA propio (dfbsa.txt: cabecera 4 B + datos + directorio al final) ────
class Bsa(object):
    """Directorio de un `.BSA`: entradas (nombre o id) → (offset, tamaño)."""

    def __init__(self, data):
        self.data = data
        if len(data) < 4:
            raise CensusError("BSA too short: %d bytes" % len(data))
        count = u16(data, 0)
        kind = u16(data, 2)
        if kind == 0x0100:
            self.record_len = 18
        elif kind == 0x0200:
            self.record_len = 8
        else:
            raise CensusError("unknown BSA directory type 0x%04X" % kind)
        self.kind = kind
        dir_off = len(data) - count * self.record_len
        if dir_off < 4:
            raise CensusError("BSA directory out of bounds (offset %d)" % dir_off)
        self.by_name = {}
        self.ordered = []
        off = 4
        for i in range(count):
            base = dir_off + i * self.record_len
            size = u32(data, base + self.record_len - 4)
            if kind == 0x0100:
                name = data[base:base + 14].split(b"\0")[0].decode("latin-1")
                key = name.upper()
                if not key:
                    raise CensusError("empty BSA name at entry %d" % i)
                self.by_name[key] = (off, size)
            else:
                key = u32(data, base)
                self.by_name[key] = (off, size)
            self.ordered.append(key)
            off += size
        self.data_end = off
        self.directory_offset = dir_off
        #  Layout invariant: 4 + Σ DataSize = directory offset.
        self.layout_ok = off == dir_off

    def names(self):
        return [k for k in self.ordered if isinstance(k, str)]

    def bytes_of(self, key):
        k = key.upper() if isinstance(key, str) else key
        if k not in self.by_name:
            raise CensusError("record %r absent from the BSA" % (key,))
        off, size = self.by_name[k]
        end = off + size
        if end > len(self.data):
            raise CensusError("record %r out of bounds [%d,%d)" % (key, off, end))
        return self.data[off:end]


# ── `.RMB` (rmb.rs: layout de bloque) ─────────────────────────────────────
def parse_block_data(buf, at):
    """One `BlockData` → dict with counters, models and door count."""
    mc = u8(buf, at)
    fc = u8(buf, at + 1)
    s3 = u8(buf, at + 2)
    pc = u8(buf, at + 3)
    dc = u8(buf, at + 4)
    p = at + BLOCK_DATA_HEADER
    models = []
    for i in range(mc):
        m = p + i * MODEL_LEN
        id1 = i16(buf, m)
        id2 = u8(buf, m + 2)
        models.append({
            "model_id": (id1 & 0xFFFF) * 100 + id2,
            "object_type": u8(buf, m + 3),
            "x_pos": i32(buf, m + 36),
            "y_pos": i32(buf, m + 40),
            "z_pos": i32(buf, m + 44),
            "y_rotation": i16(buf, m + 52),
        })
    p += mc * MODEL_LEN
    p += fc * FLAT_LEN
    p += s3 * SECTION3_LEN
    p += pc * PERSON_LEN
    doors = []
    for i in range(dc):
        d = p + i * DOOR_LEN
        doors.append({
            "x_pos": i32(buf, d),
            "y_pos": i32(buf, d + 4),
            "z_pos": i32(buf, d + 8),
            "y_rotation": i16(buf, d + 12),
            "open_rotation": i16(buf, d + 14),
            "door_model_index": u8(buf, d + 16),
        })
    p += dc * DOOR_LEN
    return {
        "models": models,
        "doors": doors,
        "section3_count": s3,
        "person_count": pc,
        "consumed": p - at,
    }


def parse_rmb(buf):
    """Map Block Record → sub-records (exterior + interior) con residuo."""
    if len(buf) < RMB_HEADER_LEN:
        raise CensusError("RMB too short: %d < %d" % (len(buf), RMB_HEADER_LEN))
    count = u8(buf, 0)
    if count > MAX_SUB_RECORDS:
        raise CensusError("RMB BlockCount %d > %d" % (count, MAX_SUB_RECORDS))
    sizes = [i32(buf, BLOCK_DATA_SIZES_AT + i * 4) for i in range(MAX_SUB_RECORDS)]
    file_name = buf[6347:6347 + 13].split(b"\0")[0].decode("latin-1")
    subs = []
    cursor = RMB_HEADER_LEN
    for i in range(count):
        size = sizes[i]
        if size <= 0:
            raise CensusError("sub-record %d has size %d" % (i, size))
        ext = parse_block_data(buf, cursor)
        interior_at = cursor + ext["consumed"]
        if interior_at + BLOCK_DATA_HEADER > len(buf):
            raise CensusError("sub-record %d interior header out of bounds" % i)
        interior = parse_block_data(buf, interior_at)
        subs.append({
            "index": i,
            "declared_size": size,
            "consumed": ext["consumed"] + interior["consumed"],
            "residual": size - (ext["consumed"] + interior["consumed"]),
            "exterior": ext,
            "interior": interior,
        })
        cursor += size
        if cursor > len(buf):
            raise CensusError("sub-record %d runs past the record" % i)
    return {"file_name": file_name, "block_count": count, "subs": subs}


# ── `ARCH3D.BSA` (Arch3dFile.cs:607-663 + MeshReader.cs:645-710) ──────────
def base_archive(raw_archive):
    """`MeshReader.cs:645-648`: sólo se reduce si >100 y no es 331/156."""
    if raw_archive > 100 and raw_archive not in (331, 156):
        return raw_archive - (raw_archive // 100) * 100
    return raw_archive


def parse_mesh(rec):
    """Record 3D → (planos [ppc, texture, archive, record], nº planos base 74)."""
    if len(rec) < 64:
        raise CensusError("3D record too short: %d < 64" % len(rec))
    version = rec[0:4]
    point_count = u32(rec, 4)
    plane_count = u32(rec, 8)
    point_offset = u32(rec, 48)
    plane_offset = u32(rec, 60)
    if point_offset + point_count * 12 > len(rec):
        raise CensusError("points out of bounds: %d pts @ %d (len %d)"
                          % (point_count, point_offset, len(rec)))
    planes = []
    doors = 0
    off = plane_offset
    for _ in range(plane_count):
        if off + 8 > len(rec):
            raise CensusError("plane header out of bounds at %d (len %d)" % (off, len(rec)))
        ppc = rec[off]
        tb = u16(rec, off + 2)
        archive = base_archive(tb >> 7)
        record = tb & 0x7F
        planes.append((ppc, tb >> 7, archive, record))
        if archive == BUILDING_DOORS_ARCHIVE:
            doors += 1
        off += 8 + ppc * 8
        if off > len(rec):
            raise CensusError("plane data out of bounds at %d (len %d)" % (off, len(rec)))
    return {
        "version": version.decode("latin-1", "replace"),
        "point_count": point_count,
        "plane_count": plane_count,
        "planes": planes,
        "door_planes": doors,
    }


# ── `MAPS.BSA`: ciudades compuestas ───────────────────────────────────────
def parse_location(record, base, width_hint=None):
    """LocationExterior mínimo: puertas de location + ExteriorData (grid)."""
    door_count = u32(record, base)
    p = base + 4 + door_count * 6
    location_id = u16(record, p + 33)
    header = p + 71
    name = c_string32(record[header:header + LOCATION_NAME_LEN])
    location_type = record[header + 34]
    building_count = u16(record, header + 41)
    p = header + 48 + building_count * 26
    ext = record[p:p + 416]
    if len(ext) < 416:
        raise CensusError("ExteriorData truncated for %r" % name)
    return {
        "location_id": location_id,
        "name": name,
        "location_type": location_type,
        "building_count": building_count,
        "location_door_elements": door_count,
        "width": ext[40],
        "height": ext[41],
        "letter1": ext[46],
        "block_index": ext[49:113],
        "block_number": ext[113:177],
        "block_character": ext[177:241],
    }


def rmb_name_parts(ext, cell):
    bi = ext["block_index"][cell]
    bn = ext["block_number"][cell]
    bc = ext["block_character"][cell]
    prefix = RMB_BLOCK_PREFIXES[bi] if bi < len(RMB_BLOCK_PREFIXES) else "FILL"
    if bi in (13, 14):
        numbers = "%s%d" % (chr((bc & 0x0F) + 65), bn)
    else:
        numbers = "%02d" % bn
    return prefix, numbers


def rmb_block_name(ext, cell):
    prefix, numbers = rmb_name_parts(ext, cell)
    bc = ext["block_character"][cell]
    letter1 = chr(ext["letter1"]) if (bc & 0x10) else "A"
    l2 = (((2 * bc) & 0xFF) >> 6)
    return "%s%s%s%s.RMB" % (prefix, letter1, LETTER2_ARRAY[l2], numbers)


def select_cities(locations):
    """Capital + primeras pequeñas (≤9 celdas, tipo 0..2, edificios > 0)."""
    selected = []
    names = [loc["name"] for loc in locations]
    if CAPITAL_CITY in names:
        selected.append(names.index(CAPITAL_CITY))
    for i, loc in enumerate(locations):
        if len(selected) >= CITIES_N:
            break
        small = 0 <= loc["location_type"] <= 2
        cells = loc["width"] * loc["height"]
        if small and 0 < cells <= SMALL_CITY_MAX_BLOCKS and loc["building_count"] > 0 \
                and i not in selected:
            selected.append(i)
    return selected


# ── censo ─────────────────────────────────────────────────────────────────
def row(name, value):
    return "row %s=%s" % (name, value)


def census(data_root, world_log):
    arena2 = os.path.join(data_root, "ARENA2")
    paths = {
        "blocks": os.path.join(arena2, "BLOCKS.BSA"),
        "arch": os.path.join(arena2, "ARCH3D.BSA"),
        "maps": os.path.join(arena2, "MAPS.BSA"),
    }
    for key, path in paths.items():
        if not os.path.isfile(path):
            raise IOError("%s absent: %s" % (key, path))
    raw = {}
    sha = {}
    for key, path in paths.items():
        with open(path, "rb") as fh:
            raw[key] = fh.read()
        sha[key] = hashlib.sha256(raw[key]).hexdigest()

    unverified = []
    rows = []
    detail = {"blocks_bsa_sha256": sha["blocks"], "arch3d_bsa_sha256": sha["arch"],
              "maps_bsa_sha256": sha["maps"],
              "blocks_bsa_bytes": len(raw["blocks"]),
              "arch3d_bsa_bytes": len(raw["arch"]),
              "maps_bsa_bytes": len(raw["maps"])}

    # ── 1. `.RMB` ────────────────────────────────────────────────────────
    blocks = Bsa(raw["blocks"])
    if not blocks.layout_ok:
        unverified.append("blocks_bsa_layout")
    rmb_names = sorted(n for n in blocks.names() if n.endswith(".RMB"))
    per_block = {}
    subrecords = 0
    doors_ext = 0
    doors_int = 0
    residual_nonzero = 0
    residual_histogram = {}
    sub_failures = 0
    for name in rmb_names:
        try:
            rec = parse_rmb(blocks.bytes_of(name))
        except CensusError as exc:
            sub_failures += 1
            unverified.append("rmb:%s(%s)" % (name, exc))
            continue
        per_block[name] = rec
        subrecords += rec["block_count"]
        for sub in rec["subs"]:
            doors_ext += len(sub["exterior"]["doors"])
            doors_int += len(sub["interior"]["doors"])
            if sub["residual"] != 0:
                residual_nonzero += 1
            residual_histogram[sub["residual"]] = residual_histogram.get(sub["residual"], 0) + 1
    if sub_failures:
        unverified.append("rmb_subrecords_%d_unreadable" % sub_failures)

    rows.append(row("blocks_bsa_sha256", sha["blocks"]))
    rows.append(row("blocks_bsa_bytes", len(raw["blocks"])))
    rows.append(row("blocks_bsa_layout_invariant", "ok" if blocks.layout_ok else "mismatch"))
    rows.append(row("rmb_files", len(rmb_names)))
    rows.append(row("rmb_subrecords", subrecords))
    rows.append(row("rmb_doors_exterior", doors_ext))
    rows.append(row("rmb_doors_interior", doors_int))
    rows.append(row("rmb_layout_residual_nonzero", residual_nonzero))
    rows.append(row("rmb_layout_residual_histogram",
                    ",".join("%d:%d" % (k, residual_histogram[k])
                             for k in sorted(residual_histogram))))

    # ── 2. `ARCH3D.BSA` ──────────────────────────────────────────────────
    arch = Bsa(raw["arch"])
    if not arch.layout_ok:
        unverified.append("arch3d_bsa_layout")
    arch_ids = [k for k in arch.ordered if not isinstance(k, str)]
    #  Unique ids by FIRST appearance: the retail directory repeats ids, and a
    #  repeated id is not another record (the duplicate count is declared).
    seen = set()
    unique_ids = []
    for rid in arch_ids:
        if rid in seen:
            continue
        seen.add(rid)
        unique_ids.append(rid)
    model_planes = {}
    door_planes_total = 0
    records_with_doors = 0
    unreadable_models = 0
    version_histogram = {}
    for rid in unique_ids:
        try:
            mesh = parse_mesh(arch.bytes_of(rid))
        except CensusError:
            unreadable_models += 1
            continue
        version_histogram[mesh["version"]] = version_histogram.get(mesh["version"], 0) + 1
        model_planes[rid] = mesh
        door_planes_total += mesh["door_planes"]
        if mesh["door_planes"]:
            records_with_doors += 1
    if unreadable_models:
        unverified.append("arch3d_records_%d_unreadable" % unreadable_models)
    rows.append(row("arch3d_bsa_sha256", sha["arch"]))
    rows.append(row("arch3d_bsa_bytes", len(raw["arch"])))
    rows.append(row("arch3d_directory_entries", len(arch_ids)))
    rows.append(row("arch3d_records", len(unique_ids)))
    rows.append(row("arch3d_duplicate_entries", len(arch_ids) - len(unique_ids)))
    rows.append(row("arch3d_records_parseable", len(model_planes)))
    rows.append(row("arch3d_versions", ",".join("%s:%d" % (v, version_histogram[v])
                                                for v in sorted(version_histogram))))
    rows.append(row("model_doors_planes_74", door_planes_total))
    rows.append(row("model_doors_records_74", records_with_doors))
    #  E-C cross-check: the two models cited in the pack.
    for probe_id in (444, 445):
        mesh = model_planes.get(probe_id)
        rows.append(row("model_%d_door_planes_74" % probe_id,
                        mesh["door_planes"] if mesh else "absent"))

    #  ── 3. what the world composes (`MAPS.BSA`) ─────────────────────────
    maps = Bsa(raw["maps"])
    location_door_elements = 0
    composed_buildings = 0
    composed_models = 0
    composed_model_doors = 0
    composed_models_unresolved = 0
    composed_blocks = []
    cities = []
    try:
        names_rec = maps.bytes_of("MAPNAMES.%s" % REGION)
        names_count = u32(names_rec, 0)
        pitem = maps.bytes_of("MAPPITEM.%s" % REGION)
        list_bytes = names_count * 4
        locations = []
        for i in range(names_count):
            off = u32(pitem, i * 4)
            locations.append(parse_location(pitem, list_bytes + off))
    except CensusError as exc:
        unverified.append("maps:%s" % exc)
        locations = []
    rows.append(row("maps_bsa_sha256", sha["maps"]))
    rows.append(row("maps_bsa_bytes", len(raw["maps"])))
    rows.append(row("maps_locations_region_017", len(locations)))
    for idx in select_cities(locations):
        loc = locations[idx]
        width, height = loc["width"], loc["height"]
        cells = width * height
        names = []
        exact = fallback = 0
        for cell in range(cells):
            wanted = rmb_block_name(loc, cell)
            if wanted.upper() in blocks.by_name:
                names.append(wanted.upper())
                exact += 1
            else:
                prefix, numbers = rmb_name_parts(loc, cell)
                suffix = "%s.RMB" % numbers
                cands = sorted(n for n in rmb_names
                               if n.startswith(prefix) and n.endswith(suffix))
                if not cands:
                    unverified.append("maps_cell:%s:%d" % (loc["name"], cell))
                    continue
                names.append(cands[0])
                fallback += 1
        city_buildings = 0
        city_doors = 0
        city_unresolved = 0
        for name in names:
            rec = per_block.get(name)
            if rec is None:
                unverified.append("composed_block_unparsed:%s" % name)
                continue
            city_buildings += rec["block_count"]
            for sub in rec["subs"]:
                for model in sub["exterior"]["models"]:
                    mesh = model_planes.get(model["model_id"])
                    if mesh is None:
                        city_unresolved += 1
                        continue
                    composed_models += 1
                    city_doors += mesh["door_planes"]
        composed_blocks.extend(names)
        composed_buildings += city_buildings
        composed_model_doors += city_doors
        composed_models_unresolved += city_unresolved
        location_door_elements += loc["location_door_elements"]
        cities.append({
            "name": loc["name"], "location_id": loc["location_id"],
            "width": width, "height": height, "cells": cells,
            "cells_exact": exact, "cells_fallback": fallback,
            "location_door_elements": loc["location_door_elements"],
            "buildings": city_buildings, "exterior_models": composed_models,
            "model_door_planes_74": city_doors,
            "unresolved_models": city_unresolved,
            "blocks": names,
        })
    detail["cities"] = cities
    detail["composed_block_names"] = sorted(set(composed_blocks))
    rows.append(row("composed_cities", len(cities)))
    rows.append(row("composed_blocks", len(composed_blocks)))
    rows.append(row("composed_buildings", composed_buildings))
    rows.append(row("composed_exterior_models", composed_models))
    rows.append(row("composed_model_doors", composed_model_doors))
    rows.append(row("composed_models_unresolved", composed_models_unresolved))
    rows.append(row("location_door_elements", location_door_elements))
    #  The world composes the CAPITAL alone (`compose_capital_with_pixel`), not the 5
    #  cities of `PROBE-RMB-001`: both figures are declared so that the
    #  census↔world cross-check (step 3) has a checkable equality.
    capital = next((c for c in cities if c["name"] == CAPITAL_CITY), None)
    if capital is None:
        unverified.append("capital_absent_from_selection")
    else:
        rows.append(row("capital_blocks", len(capital["blocks"])))
        rows.append(row("capital_buildings", capital["buildings"]))
        rows.append(row("capital_exterior_models", capital["exterior_models"]))
        rows.append(row("capital_model_doors", capital["model_door_planes_74"]))
        rows.append(row("capital_location_door_elements", capital["location_door_elements"]))

    #  ── 4. what reaches the scene today (app instrument) ────────────────
    world = None
    if world_log:
        try:
            with open(world_log, "r", encoding="utf-8", errors="replace") as fh:
                for line in fh:
                    if "FG01-02 VE: world doors composed=" in line:
                        world = line.strip()
        except OSError as exc:
            unverified.append("world_log:%s" % exc)
        if world is None:
            unverified.append("world_line_absent_in_%s" % os.path.basename(world_log))
        else:
            try:
                parts = dict(p.split("=", 1) for p in world.split()[4:] if "=" in p)
                rows.append(row("world_doors_composed", parts["composed"]))
                rows.append(row("world_doors_today", parts["retail"]))
                rows.append(row("world_doors_scenario", parts["scenario"]))
            except (KeyError, IndexError):
                unverified.append("world_line_unparsable")
    else:
        unverified.append("world_doors_today_no_instrument_log")
        rows.append(row("world_doors_today", "not-measured"))
    if world:
        rows.append(row("world_line", world.replace(" ", "_")))

    rows.append(row("unverified", ",".join(unverified) if unverified else "none"))
    rows.append(row("result", "COMPLETE" if not unverified else "UNVERIFIED"))
    detail["rows"] = rows
    detail["unverified"] = unverified
    return rows, detail, unverified


def write_detail(out_dir, detail):
    os.makedirs(out_dir, exist_ok=True)
    path = os.path.join(out_dir, "census.json")
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(detail, fh, indent=2, sort_keys=True)
        fh.write("\n")
    return path


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--data-root", default=DEFAULT_DATA_ROOT, help="root de datos retail")
    ap.add_argument("--out", default=DEFAULT_OUT, help="dir de detalle (gitignored)")
    ap.add_argument("--json", action="store_true", help="acta JSON en stdout")
    ap.add_argument("--world", default=None,
                    help="log de la app con `FG01-02 VE: world doors …`")
    args = ap.parse_args(argv)

    data_root = os.path.abspath(args.data_root)
    if not os.path.isdir(os.path.join(data_root, "ARENA2")):
        print("row error=no-arena2\nrow result=FAIL")
        print("IO: %s/ARENA2 no existe (--data-root)" % data_root, file=sys.stderr)
        return 2
    try:
        rows, detail, unverified = census(data_root, args.world)
    except (CensusError, IOError, OSError) as exc:
        print("row error=%s\nrow result=FAIL" % str(exc).split("\n")[0])
        print("IO: %s" % exc, file=sys.stderr)
        return 2
    try:
        path = write_detail(os.path.abspath(args.out), detail)
    except OSError as exc:
        print("row error=out-dir\nrow result=FAIL")
        print("IO: %s" % exc, file=sys.stderr)
        return 2

    if args.json:
        print(json.dumps({"rows": rows, "detail": detail}, indent=2, sort_keys=True))
    else:
        print("\n".join(rows))
        print("detail: %s" % path)
    return 0 if not unverified else 1


if __name__ == "__main__":
    sys.exit(main())
