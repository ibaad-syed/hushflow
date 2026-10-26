"""Compares llama-server configurations for cleanup: load time, memory, latency, quality.

Usage: python3 cleanup_perf.py <llama-server> <model.gguf> [config-name ...]
Each config is a flag set (see CONFIGS). Memory is the process's physical
footprint (what Activity Monitor shows), not RSS, which also counts
reclaimable mmapped file pages. macOS only (uses `footprint`).
"""
import json
import os
import re
import subprocess
import sys
import time
import urllib.request

import cleanup_eval as ev

# The app's flags before this round of tuning (server.rs).
BASE = ["-ngl", "99", "-c", "4096", "-np", "1", "--jinja", "--cache-reuse", "256",
        "--no-webui", "--spec-type", "ngram-simple",
        "--spec-ngram-simple-size-n", "3", "--spec-ngram-simple-size-m", "16"]

CONFIGS = {
    "base": BASE,
    "lazy": BASE + ["--lazy-mode", "on"],
    "cram0": BASE + ["--cache-ram", "0"],
    "fa": BASE + ["-fa", "on"],
    "lazy+cram0": BASE + ["--lazy-mode", "on", "--cache-ram", "0"],
    "tuned": BASE + ["--lazy-mode", "on", "--cache-ram", "0", "-fa", "on", "--no-warmup"],
}


def footprint_mb(pid):
    out = subprocess.run(["footprint", "-p", str(pid)], capture_output=True, text=True).stdout
    m = re.search(r"Footprint:\s*([\d.]+)\s*(KB|MB|GB)", out)
    if not m:
        return -1
    n, unit = float(m.group(1)), m.group(2)
    return round(n * {"KB": 1 / 1024, "MB": 1, "GB": 1024}[unit])


def run(server, model, name, cases):
    t0 = time.perf_counter()
    proc = subprocess.Popen([server, "-m", model, "--port", str(ev.PORT), *CONFIGS[name]],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        ev.wait_ready(proc)
        ready_ms = (time.perf_counter() - t0) * 1000
        first_ms, _ = ev.call("Warm up the cache, please.")
        mem_warm = footprint_mb(proc.pid)
        rows = []
        for case in cases:
            ms, raw = ev.call(case["input"])
            out, guarded = ev.guard(case["input"], raw)
            rows.append({"ms": ms, "pass": not ev.score(case, out)})
        mem_after = footprint_mb(proc.pid)
        ms = sorted(r["ms"] for r in rows)
        return {"config": name, "ready_ms": round(ready_ms), "first_ms": round(first_ms),
                "mem_warm_mb": mem_warm, "mem_after_mb": mem_after,
                "pass": sum(r["pass"] for r in rows), "n": len(rows),
                "p50": round(ms[len(ms) // 2]), "p90": round(ms[int(len(ms) * .9)])}
    finally:
        proc.terminate()
        proc.wait()


def main():
    server, model, names = sys.argv[1], sys.argv[2], sys.argv[3:] or list(CONFIGS)
    cases = json.load(open(os.path.join(ev.HERE, "cleanup_eval.json")))
    results = []
    for name in names:
        r = run(server, model, name, cases)
        results.append(r)
        print(json.dumps(r), flush=True)
    print(f"\n{'config':12} {'ready':>6} {'1st':>5} {'mem':>6} {'mem+':>6} {'pass':>6} {'p50':>5} {'p90':>5}")
    for r in results:
        print(f"{r['config']:12} {r['ready_ms']:>6} {r['first_ms']:>5} {r['mem_warm_mb']:>6} "
              f"{r['mem_after_mb']:>6} {r['pass']:>3}/{r['n']:<2} {r['p50']:>5} {r['p90']:>5}")


if __name__ == "__main__":
    main()
