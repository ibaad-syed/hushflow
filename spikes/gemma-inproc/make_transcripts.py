"""Synthetic meeting transcripts, built like the earlier prefill bench:
random cleanup-eval utterances with rising [mm:ss] stamps and Me/Them speakers.

    python3 -I make_transcripts.py <cleanup_eval.json> <out_dir>

Writes transcript_<words>.txt for 1500, 5000 and 10000 words.
"""
import json
import random
import sys

cases = json.load(open(sys.argv[1]))
out = sys.argv[2]
random.seed(1)
lines, t = [], 0
while len(" ".join(lines).split()) < 10_500:
    c = random.choice(cases)
    t += random.randint(4, 25)
    stamp = f"{t // 60:02d}:{t % 60:02d}" if t < 3600 else f"{t // 3600}:{t // 60 % 60:02d}:{t % 60:02d}"
    lines.append(f"[{stamp}] {random.choice(['Me', 'Them'])}: {c['input']}")

for n in (1500, 5000, 10000):
    keep, w = [], 0
    for line in lines:
        keep.append(line)
        w += len(line.split())
        if w >= n:
            break
    with open(f"{out}/transcript_{n}.txt", "w") as f:
        f.write("\n".join(keep))
    print(n, "words ->", len(keep), "lines, last", keep[-1][:12])
