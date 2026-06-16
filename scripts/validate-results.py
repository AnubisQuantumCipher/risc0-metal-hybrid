#!/usr/bin/env python3
"""Validate results/*.json against the r0mh-results-v1 schema AND the
zero-fabrication invariants that the schema alone cannot express.

Usage:
  python3 scripts/validate-results.py                 # all results/*.json
  python3 scripts/validate-results.py results/foo.json [more.json ...]

Exit code is non-zero iff any file fails. The structural/internal-consistency
checks here are necessary, not sufficient: a 'measured' row's timings are only
trustworthy because a real evidence bundle (gitignored / release asset) backs
them and a human reviewed it. This validator guards against the failure modes a
machine CAN catch: schema drift, a placeholder masquerading as data, an
unverified receipt, a speedup that does not match its own timings, a chip_key
that does not match its filename, and — as of the per-workload provenance change
— a measured row missing its own evidence block or carrying a malformed/blank
provenance hash. A null CSV/profile hash is allowed only with a reason in notes;
no hash is invented.
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SCHEMA_PATH = os.path.join(ROOT, "results", "schema", "r0mh-results-v1.schema.json")
RESULTS_DIR = os.path.join(ROOT, "results")
SPEEDUP_TOL = 0.02  # |reported - cpu/metal| / (cpu/metal) must be within 2%

WORKLOAD_IDS = {"hello", "busy", "hash", "ecdsa", "shaheavy", "mempress", "multiseg"}
HEX64 = re.compile(r"[a-f0-9]{64}")


def is_hex64(v):
    """True iff v is a 64-char lowercase-hex string. Type-guards before the
    regex so a non-string (e.g. a JSON number) yields a clean validation error
    rather than a TypeError from fullmatch()."""
    return isinstance(v, str) and HEX64.fullmatch(v) is not None


def workload_evidence_errs(name, ev):
    """Per-workload provenance invariants the schema cannot fully express: the
    row must point at a bundle, carry a 64-hex evidence-JSON hash, and carry the
    three per-lane/profile hashes as EITHER 64 hex OR null — and a null is only
    honest if 'notes' explains why (the file is absent in that bundle). No hash
    is ever invented; that is enforced here as 'present-and-well-formed', and at
    review time as 'the named file in the cited bundle hashes to this value'."""
    errs = []
    if not isinstance(ev, dict) or not ev:
        return [f"{name}: missing per-workload 'evidence' block (every measured workload must carry one)"]
    if not ev.get("bundle"):
        errs.append(f"{name}/evidence: must point at a 'bundle'")
    if not is_hex64(ev.get("evidence_json_sha256")):
        errs.append(f"{name}/evidence: evidence_json_sha256 must be 64 hex chars (the bundle's evidence.json or summary.json)")
    notes = ev.get("notes", "") or ""
    if not notes.strip():
        errs.append(f"{name}/evidence: must carry a non-empty 'notes' provenance line")
    for key in ("metal_csv_sha256", "cpu_csv_sha256", "profile_log_sha256"):
        if key not in ev:
            errs.append(f"{name}/evidence: missing '{key}' (use null with a reason in notes if the file is absent)")
            continue
        val = ev[key]
        if val is None:
            # A null is only honest if notes say why; require a non-trivial note.
            if not notes.strip():
                errs.append(f"{name}/evidence: {key} is null but notes does not explain why")
        elif not is_hex64(val):
            errs.append(f"{name}/evidence: {key} must be 64 hex chars or null, got {val!r}")
    return errs


def load(path):
    with open(path, "r", encoding="utf-8") as fh:
        return json.load(fh)


def schema_validate(doc, schema):
    """Validate against the JSON Schema if jsonschema is installed; otherwise
    return [] and let the structural checks below do the work."""
    try:
        import jsonschema  # type: ignore
    except ImportError:
        return ["(jsonschema not installed — schema check skipped; structural checks still run)"]
    validator = jsonschema.Draft202012Validator(schema)
    return [f"schema: {e.json_path}: {e.message}" for e in validator.iter_errors(doc)]


def structural_checks(path, doc):
    errs = []
    stem = os.path.splitext(os.path.basename(path))[0]

    if doc.get("schema_version") != "r0mh-results-v1":
        errs.append("schema_version must be 'r0mh-results-v1'")
    if doc.get("chip_key") != stem:
        errs.append(f"chip_key '{doc.get('chip_key')}' != filename stem '{stem}'")

    status = doc.get("status")
    workloads = doc.get("workloads", [])

    if status == "not_measured":
        # A placeholder must carry NO timings and an honest reason.
        if workloads:
            errs.append("not_measured row must have an empty 'workloads' (no timings)")
        if not doc.get("not_measured_reason"):
            errs.append("not_measured row must carry 'not_measured_reason'")
    elif status == "measured":
        if not workloads:
            errs.append("measured row must have at least one workload")
        ev = doc.get("evidence", {})
        h = ev.get("evidence_json_sha256", "")
        if not re.fullmatch(r"[a-f0-9]{64}", h or ""):
            errs.append("measured row must carry evidence.evidence_json_sha256 (64 hex chars)")
        if not ev.get("bundle"):
            errs.append("measured row must point at evidence.bundle")
        if ev.get("verdict") not in (None, "PASS"):
            errs.append("a row whose evidence verdict is not PASS must not be published")
        for w in workloads:
            name = w.get("name")
            if name not in WORKLOAD_IDS:
                errs.append(f"unknown workload id '{name}'")
            for lane in ("metal", "cpu"):
                ln = w.get(lane, {})
                if ln.get("receipt_verified") is not True:
                    errs.append(f"{name}/{lane}: receipt_verified must be true (an unverified run is not a result)")
                if not isinstance(ln.get("median_ms"), (int, float)) or ln.get("median_ms", -1) < 0:
                    errs.append(f"{name}/{lane}: median_ms must be a non-negative number")
            # Per-workload provenance: every measured row pins its own files.
            errs.extend(workload_evidence_errs(name, w.get("evidence")))
            # speedup must match the row's own timings (no hand-entered ratio).
            try:
                m = float(w["metal"]["median_ms"])
                c = float(w["cpu"]["median_ms"])
                if m > 0 and "speedup" in w:
                    ratio = c / m
                    rep = float(w["speedup"])
                    if abs(rep - ratio) / ratio > SPEEDUP_TOL:
                        errs.append(
                            f"{name}: speedup {rep} != cpu/metal {ratio:.3f} "
                            f"(>{int(SPEEDUP_TOL*100)}% off — numbers must agree)"
                        )
            except (KeyError, ZeroDivisionError, TypeError, ValueError):
                pass
    else:
        errs.append(f"status must be 'measured' or 'not_measured', got {status!r}")

    return errs


def main(argv):
    schema = load(SCHEMA_PATH)
    if len(argv) > 1:
        files = argv[1:]
    else:
        files = sorted(
            os.path.join(RESULTS_DIR, f)
            for f in os.listdir(RESULTS_DIR)
            if f.endswith(".json")
        )
    if not files:
        print("no results/*.json found", file=sys.stderr)
        return 1

    total_errs = 0
    for path in files:
        try:
            doc = load(path)
        except (OSError, json.JSONDecodeError) as e:
            print(f"FAIL {path}: cannot load: {e}")
            total_errs += 1
            continue
        errs = schema_validate(doc, schema) + structural_checks(path, doc)
        hard = [e for e in errs if not e.startswith("(")]
        soft = [e for e in errs if e.startswith("(")]
        for s in soft:
            print(f"note {os.path.basename(path)}: {s}")
        if hard:
            total_errs += len(hard)
            print(f"FAIL {os.path.basename(path)} ({doc.get('status','?')}):")
            for e in hard:
                print(f"  - {e}")
        else:
            print(f"ok   {os.path.basename(path)} ({doc.get('status','?')})")

    print(f"\n{len(files)} file(s), {total_errs} error(s)")
    return 1 if total_errs else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
