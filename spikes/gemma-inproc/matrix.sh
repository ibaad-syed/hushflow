#!/bin/bash
# The full measurement matrix behind docs/fork/bench/gemma_inproc.md.
#   matrix.sh <data_dir> <model.gguf> <tag> [quick]
# data_dir holds transcript_{1500,5000,10000}.txt (make_transcripts.py).
# "quick" runs only the subset used for an alternative quant.
set -uo pipefail
d=$1; m=$2; tag=$3; quick=${4:-}
b="$(dirname "$0")/bench.sh"
t() { echo "$d/transcript_$1.txt"; }
run() { "$b" "$@" || echo "$1 FAILED" | tee -a "$d/results.txt"; }

if [ "$quick" = core ]; then
  # the headline rows only (re-measured on an otherwise idle machine)
  for w in 1500 5000 10000; do run "${tag}_metal_$w" "$d" --model "$m" --transcript "$(t $w)"; done
  run "${tag}_metal_nommap_10000" "$d" --model "$m" --transcript "$(t 10000)" --no-mmap
  for w in 1500 5000 10000; do run "${tag}_cpu_$w" "$d" --model "$m" --transcript "$(t $w)" --cpu; done
elif [ -z "$quick" ]; then
  # (a) Metal, full offload, context sized as notes.rs does
  for w in 1500 5000 10000; do run "${tag}_metal_$w" "$d" --model "$m" --transcript "$(t $w)"; done
  for w in 1500 10000; do run "${tag}_metal_nommap_$w" "$d" --model "$m" --transcript "$(t $w)" --no-mmap; done
  # (b) CPU only, 4 threads
  for w in 1500 5000 10000; do run "${tag}_cpu_$w" "$d" --model "$m" --transcript "$(t $w)" --cpu; done
  for w in 1500 10000; do run "${tag}_cpu_nommap_$w" "$d" --model "$m" --transcript "$(t $w)" --cpu --no-mmap; done
  # context size and KV type, 1.5k words, weights resident so deltas show
  for c in 8192 16384 32768; do
    run "${tag}_metal_nommap_ctx${c}_1500" "$d" --model "$m" --transcript "$(t 1500)" --no-mmap --ctx $c
    run "${tag}_metal_nommap_ctx${c}_q8_1500" "$d" --model "$m" --transcript "$(t 1500)" --no-mmap --ctx $c --kv q8_0
  done
  run "${tag}_cpu_nommap_ctx16384_1500" "$d" --model "$m" --transcript "$(t 1500)" --cpu --no-mmap --ctx 16384
  run "${tag}_cpu_nommap_ctx16384_q8_1500" "$d" --model "$m" --transcript "$(t 1500)" --cpu --no-mmap --ctx 16384 --kv q8_0
  run "${tag}_metal_q8_10000" "$d" --model "$m" --transcript "$(t 10000)" --kv q8_0
  # smaller compute buffer
  run "${tag}_metal_nommap_ub256_1500" "$d" --model "$m" --transcript "$(t 1500)" --no-mmap --ubatch 256
  run "${tag}_cpu_nommap_ub256_1500" "$d" --model "$m" --transcript "$(t 1500)" --cpu --no-mmap --ubatch 256
  # greedy, for the engine-equivalence check against llama-server
  run "${tag}_metal_greedy_1500" "$d" --model "$m" --transcript "$(t 1500)" --greedy
else
  for w in 1500 10000; do
    run "${tag}_metal_$w" "$d" --model "$m" --transcript "$(t $w)"
    run "${tag}_metal_nommap_$w" "$d" --model "$m" --transcript "$(t $w)" --no-mmap
    run "${tag}_cpu_$w" "$d" --model "$m" --transcript "$(t $w)" --cpu
  done
  run "${tag}_cpu_nommap_1500" "$d" --model "$m" --transcript "$(t 1500)" --cpu --no-mmap
fi
echo "MATRIX DONE $tag"
