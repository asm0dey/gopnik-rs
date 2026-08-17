#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GHIDRA=/opt/ghidra/support/analyzeHeadless
PROJ="$ROOT/ghidra_proj"
OUT="$ROOT/build"

mkdir -p "$PROJ" "$OUT"

if [ ! -d "$PROJ/gopnik.rep" ]; then
  "$GHIDRA" "$PROJ" gopnik -import "$ROOT/orig/g.exe" -analysisTimeoutPerFile 600
fi

"$GHIDRA" "$PROJ" gopnik -process g.exe -noanalysis \
  -scriptPath "$ROOT/tools/ghidra" \
  -postScript ExportAll.java "$OUT" \
  -postScript DumpImmediates.java "$OUT" "$ROOT/orig/g.exe"

cp "$OUT/functions.json" "$ROOT/data/functions.json"
cp "$OUT/string_pointers.json" "$ROOT/data/string_pointers.json"
echo "decomp files: $(ls "$OUT/decomp" | wc -l)"
