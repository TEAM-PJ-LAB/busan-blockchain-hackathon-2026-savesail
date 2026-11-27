"""PRD S5/S6: does each video script's narration fit its time slots and the length limit?

Reads the tables in docs/video-scripts.md (| time | screen | narration |). Spoken length is estimated as syllables at
4.5 per second: Hangul syllables, digits one each, Latin words by letters / 2, '%' 3 and '%p' 6; number markers and
[README n] citations are not spoken. Each row must end before the next row starts, the last row before the limit.
Usage: python3 scripts/check_script_time.py [FILE]
"""
import math
import re
import sys

RATE = 4.5
LIMITS = {"S5": 180, "S6": 150}


def syllables(text):
    text = re.sub(r"<!--.*?-->", "", text)
    text = re.sub(r"\[README [^\]]*\]", "", text)
    text = text.replace("`", "")
    n = len(re.findall(r"[가-힣]", text)) + len(re.findall(r"\d", text))
    n += 6 * text.count("%p") + 3 * (text.count("%") - text.count("%p"))
    n += sum(math.ceil(len(w) / 2) for w in re.findall(r"[A-Za-z_]+", text))
    return n


def secs(t):
    m, s = t.split(":")
    return int(m) * 60 + int(s)


def main(path="docs/video-scripts.md"):
    fails, section, rows = [], None, {}
    for line in open(path, encoding="utf-8"):
        h = re.match(r"## (S\d)\.", line)
        if h:
            section = h.group(1)
            rows[section] = []
        m = re.match(r"\| (\d+:\d\d) \| (.*?) \| (.*) \|$", line.rstrip("\n"))
        if m and section:
            rows[section].append((secs(m.group(1)), syllables(m.group(3))))
    for sec, rs in rows.items():
        limit = LIMITS[sec]
        total = 0.0
        for i, (start, syl) in enumerate(rs):
            end = rs[i + 1][0] if i + 1 < len(rs) else limit
            need = syl / RATE
            total += need
            if start + need > end:
                fails.append(f"{sec} {start // 60}:{start % 60:02d}: {need:.1f}s of speech in a {end - start}s slot")
        print(f"{sec}: {sum(s for _, s in rs)} syllables, {total:.0f}s of speech, limit {limit}s")
    for f in fails:
        print("FAIL", f)
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:]))
