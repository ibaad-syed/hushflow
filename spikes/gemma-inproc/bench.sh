#!/bin/bash
# One measured run: gemma-inproc under /usr/bin/time -l, printing the RESULT
# line plus max RSS and peak physical footprint (what iOS jetsam counts).
#   bench.sh <label> <out_dir> <gemma-inproc args...>
set -euo pipefail
label=$1; out=$2; shift 2
bin="${BIN:-$(dirname "$0")/target/release/gemma-inproc}"
log="$out/$label.log"
/usr/bin/time -l "$bin" --out "$out/$label.md" "$@" 2> "$log"
res=$(grep '^RESULT' "$log")
rss=$(awk '/maximum resident set size/ {print int($1/1048576)}' "$log")
fp=$(awk '/peak memory footprint/ {print int($1/1048576)}' "$log")
real=$(awk '/ real / {print $1}' "$log")
echo "$label $res rss_mb=$rss footprint_mb=$fp wall_s=$real" | tee -a "$out/results.txt"
