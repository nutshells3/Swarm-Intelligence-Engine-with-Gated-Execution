#!/usr/bin/env bash
# generate_drift_reports.sh — Regenerate all drift/duplicate/cross-ref reports
#
# Usage: ./scripts/generate_drift_reports.sh
#
# Requires: running postgres (docker-compose up), python3

set -euo pipefail

REFACT_DIR="refact"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "=== 1. SQL Schema Dump ==="
docker exec first-postgres-1 psql -U postgres -d development_swarm \
    -c "SELECT table_name, column_name, data_type, is_nullable, column_default FROM information_schema.columns WHERE table_schema='public' ORDER BY table_name, ordinal_position" \
    --csv > "$REFACT_DIR/SQL_SCHEMA_DUMP.csv"
echo "  $(wc -l < "$REFACT_DIR/SQL_SCHEMA_DUMP.csv") lines"

echo "=== 2. Rust Pub Signatures ==="
grep -rn "^pub struct\|^pub enum\|^pub trait\|^pub type\|^pub fn\|^pub async fn\|^    pub fn\|^    pub async fn" \
    --include="*.rs" packages/ services/ apps/cli/ apps/desktop/src-tauri/ \
    | grep -v target | grep -v .worktrees | sort > "$REFACT_DIR/RUST_SIGNATURES_RAW.txt"
echo "  $(wc -l < "$REFACT_DIR/RUST_SIGNATURES_RAW.txt") signatures"

echo "=== 3. Rust Types With Fields (JSON) ==="
python3 << 'PYEOF'
import re, os, json
results = {}
root = os.environ.get("ROOT", ".")
for dirpath, dirs, files in os.walk(root):
    if "target" in dirpath or ".worktrees" in dirpath or "node_modules" in dirpath:
        continue
    for f in files:
        if not f.endswith(".rs"):
            continue
        fpath = os.path.join(dirpath, f)
        rel = os.path.relpath(fpath, root).replace(os.sep, "/")
        try:
            with open(fpath, "r", encoding="utf-8") as fh:
                content = fh.read()
        except:
            continue
        for m in re.finditer(r"pub struct (\w+)\s*\{([^}]*)\}", content, re.DOTALL):
            name = m.group(1)
            body = m.group(2)
            fields = []
            for line in body.strip().split("\n"):
                line = line.strip().rstrip(",")
                fm = re.match(r"pub (\w+)\s*:\s*(.+)", line)
                if fm:
                    fields.append({"name": fm.group(1), "type": fm.group(2).strip()})
            if fields:
                results[rel + "::" + name] = fields
        for m in re.finditer(r"pub enum (\w+)\s*\{([^}]*)\}", content, re.DOTALL):
            name = m.group(1)
            body = m.group(2)
            variants = []
            for line in body.strip().split("\n"):
                line = line.strip().rstrip(",")
                if line.startswith("//") or line.startswith("#") or not line:
                    continue
                vm = re.match(r"(\w+)", line)
                if vm:
                    variants.append(vm.group(1))
            if variants:
                results[rel + "::enum::" + name] = variants
outpath = os.path.join(root, "refact", "RUST_TYPES_WITH_FIELDS.json")
with open(outpath, "w") as out:
    json.dump(results, out, indent=2)
print(f"  {len(results)} type definitions extracted")
PYEOF

echo "=== 4. Verified Duplicates ==="
python3 << 'PYEOF'
import json, os
root = os.environ.get("ROOT", ".")
with open(os.path.join(root, "refact", "RUST_TYPES_WITH_FIELDS.json")) as f:
    rust_types = json.load(f)
dupes = {}
for key in rust_types:
    parts = key.split("::")
    is_enum = "enum" in parts
    name = parts[-1]
    tag = f"enum::{name}" if is_enum else name
    if tag not in dupes:
        dupes[tag] = []
    dupes[tag].append(key)
report = ["# Verified Duplicate Analysis (Machine-Generated)", ""]
count = 0
for tag, locations in sorted(dupes.items()):
    if len(locations) < 2:
        continue
    count += 1
    name = tag.split("::")[-1]
    report.append(f"### {name} ({len(locations)} definitions)")
    report.append("")
    for loc in locations:
        pkg = loc.split("/src/")[0] if "/src/" in loc else loc
        fields = rust_types[loc]
        if isinstance(fields, list) and len(fields) > 0:
            if isinstance(fields[0], dict):
                field_str = ", ".join(f"{f['name']}: {f['type']}" for f in fields)
            else:
                field_str = ", ".join(str(v) for v in fields)
        else:
            field_str = "(empty)"
        report.append(f"- **{pkg}**: `{field_str}`")
    field_sets = []
    for loc in locations:
        fields = rust_types[loc]
        if isinstance(fields, list) and len(fields) > 0:
            if isinstance(fields[0], dict):
                fs = frozenset((f["name"], f["type"]) for f in fields)
            else:
                fs = frozenset(str(v) for v in fields)
        else:
            fs = frozenset()
        field_sets.append(fs)
    if len(set(field_sets)) == 1:
        report.append("- VERDICT: IDENTICAL")
    else:
        report.append("- VERDICT: DIVERGED")
    report.append("")
with open(os.path.join(root, "refact", "VERIFIED_DUPLICATES.md"), "w") as f:
    f.write("\n".join(report))
print(f"  {count} duplicate type names found")
PYEOF

echo "=== 5. SQL-Rust Cross-Reference ==="
python3 << 'PYEOF'
import json, csv, os
root = os.environ.get("ROOT", ".")
with open(os.path.join(root, "refact", "RUST_TYPES_WITH_FIELDS.json")) as f:
    rust_types = json.load(f)
sql_schema = {}
with open(os.path.join(root, "refact", "SQL_SCHEMA_DUMP.csv"), newline="") as f:
    reader = csv.reader(f)
    header = next(reader)
    for row in reader:
        tbl = row[0]
        if tbl == "_sqlx_migrations":
            continue
        if tbl not in sql_schema:
            sql_schema[tbl] = []
        sql_schema[tbl].append({"column": row[1], "type": row[2], "nullable": row[3]})
def snake_to_pascal(s):
    return "".join(w.capitalize() for w in s.split("_"))
matches = []
for tbl_name, cols in sql_schema.items():
    pascal = snake_to_pascal(tbl_name)
    for key, fields in rust_types.items():
        if "::enum::" in key:
            continue
        if not isinstance(fields, list) or not fields or not isinstance(fields[0], dict):
            continue
        type_name = key.split("::")[-1]
        singular = pascal.rstrip("s")
        if type_name in [pascal, pascal+"Record", pascal+"Row", singular, singular+"Record"]:
            sql_cols = set(c["column"] for c in cols)
            rust_fields = set(f["name"] for f in fields)
            matches.append({
                "table": tbl_name, "rust_type": key,
                "matched": len(sql_cols & rust_fields),
                "only_sql": sorted(sql_cols - rust_fields),
                "only_rust": sorted(rust_fields - sql_cols),
                "total_sql": len(sql_cols), "total_rust": len(rust_fields)
            })
report = ["# SQL-to-Rust Cross-Reference (Machine-Generated)", "",
          f"SQL tables: {len(sql_schema)} | Matched pairs: {len(matches)}", ""]
for m in sorted(matches, key=lambda x: (-len(x["only_sql"]), x["table"])):
    status = "OK" if not m["only_sql"] and not m["only_rust"] else "DRIFT"
    tn = m["rust_type"].split("::")[-1]
    report.append(f"### {m['table']} <-> {tn} [{status}]")
    report.append(f"- Matched: {m['matched']}/{m['total_sql']} SQL, {m['matched']}/{m['total_rust']} Rust")
    if m["only_sql"]:
        report.append(f"- SQL only: {', '.join(m['only_sql'])}")
    if m["only_rust"]:
        report.append(f"- Rust only: {', '.join(m['only_rust'])}")
    report.append("")
matched_tables = set(m["table"] for m in matches)
unmatched = sorted(set(sql_schema.keys()) - matched_tables)
report.append(f"## Unmatched SQL Tables ({len(unmatched)})")
report.append("")
for t in unmatched:
    cols = ", ".join(c["column"] for c in sql_schema[t])
    report.append(f"- **{t}** ({len(sql_schema[t])} cols): {cols}")
with open(os.path.join(root, "refact", "SQL_RUST_CROSSREF.md"), "w") as f:
    f.write("\n".join(report))
print(f"  {len(matches)} matched, {len(unmatched)} unmatched tables")
PYEOF

echo "=== 6. TS-Rust Cross-Reference ==="
python3 << 'PYEOF'
import json, re, os
root = os.environ.get("ROOT", ".")
ts_path = os.path.join(root, "apps/web/src/types/api.ts")
with open(ts_path, "r") as f:
    ts_content = f.read()
ts_types = {}
for m in re.finditer(r"export interface (\w+)\s*\{([^}]*)\}", ts_content, re.DOTALL):
    name = m.group(1)
    body = m.group(2)
    fields = []
    for line in body.strip().split("\n"):
        line = line.strip().rstrip(";").rstrip(",")
        fm = re.match(r"(\w+)\??\s*:\s*(.+)", line)
        if fm:
            fields.append({"name": fm.group(1), "type": fm.group(2).strip()})
    ts_types[name] = fields
with open(os.path.join(root, "refact", "RUST_TYPES_WITH_FIELDS.json")) as f:
    rust_types = json.load(f)
api_rust = {}
for key, fields in rust_types.items():
    if "orchestration-api" in key and "::enum::" not in key and isinstance(fields, list) and fields and isinstance(fields[0], dict):
        name = key.split("::")[-1]
        if name not in api_rust:
            api_rust[name] = fields
report = ["# TypeScript-to-Rust Cross-Reference (Machine-Generated)", ""]
for ts_name, ts_fields in sorted(ts_types.items()):
    rust_match = api_rust.get(ts_name)
    if rust_match:
        ts_fn = set(f["name"] for f in ts_fields)
        rust_fn = set(f["name"] for f in rust_match)
        status = "OK" if not (ts_fn - rust_fn) and not (rust_fn - ts_fn) else "DRIFT"
        report.append(f"### {ts_name} [{status}]")
        report.append(f"- Matched: {len(ts_fn & rust_fn)}/{len(ts_fn)} TS, {len(ts_fn & rust_fn)}/{len(rust_fn)} Rust")
        if ts_fn - rust_fn:
            report.append(f"- TS only: {', '.join(sorted(ts_fn - rust_fn))}")
        if rust_fn - ts_fn:
            report.append(f"- Rust only: {', '.join(sorted(rust_fn - ts_fn))}")
    else:
        report.append(f"### {ts_name} [NO RUST MATCH]")
        report.append(f"- Fields: {', '.join(f['name'] for f in ts_fields)}")
    report.append("")
with open(os.path.join(root, "refact", "TS_RUST_CROSSREF.md"), "w") as f:
    f.write("\n".join(report))
print(f"  {len(ts_types)} TS types checked")
PYEOF

echo "=== Done ==="
