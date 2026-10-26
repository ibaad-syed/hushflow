"""Shape and grounding check for generated notes, like notes.rs's
check_citations: every cited [mm:ss] must be within 2 s of a transcript line.

    python3 -I check_notes.py <transcript.txt> <notes.md> [<notes.md> ...]
"""
import re
import sys

STAMP = re.compile(r"(?:(\d{1,2}):)?(\d{1,2}):(\d{2})")
SECTIONS = ["## Summary", "## Notes", "## Decisions", "## Action items", "## Open questions"]


def secs(m):
    h = int(m.group(1) or 0)
    return (h * 60 + int(m.group(2))) * 60 + int(m.group(3))


starts = []
for line in open(sys.argv[1]):
    m = STAMP.match(line.strip().lstrip("["))
    if m:
        starts.append(secs(m))

for path in sys.argv[2:]:
    text = open(path).read()
    cites = [secs(m) for b in re.findall(r"\[([^\]]*\d:\d\d[^\]]*)\]", text) for m in STAMP.finditer(b)]
    bad = [c for c in cites if not any(abs(c - s) <= 2 for s in starts)]
    shape = text.startswith("# ") and all(s in text for s in SECTIONS)
    print(f"{path.rsplit('/', 1)[-1]}: shape={'ok' if shape else 'BROKEN'} "
          f"words={len(text.split())} citations={len(cites)} unverified={len(bad)}")
