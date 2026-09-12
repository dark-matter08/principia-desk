#!/usr/bin/env python3
"""Replace the em dashes in bundled text the way the desk's prose scrub
does at runtime (src-tauri/src/prose.rs): outside code, an em dash or a
spaced en dash becomes a comma, a dash after a mark that already breaks
the clause keeps the mark, one at the end of a line disappears.

Usage: scripts/scrub-dashes.py <files...>   (.txt, .md, .json)
JSON files are rewritten with two-space indentation, string values only.
"""
import json
import sys
from pathlib import Path


def parts(text):
    out, rest = [], text
    while rest:
        fence = rest.find("```")
        tick = rest.find("`")
        if tick == fence:
            tick = -1
        if fence >= 0 and (tick < 0 or fence < tick):
            out.append((False, rest[:fence]))
            end = rest.find("```", fence + 3)
            if end < 0:  # a fence that never closes is a mention of one
                out.append((False, rest[fence:fence + 3]))
                rest = rest[fence + 3:]
            else:
                out.append((True, rest[fence:end + 3]))
                rest = rest[end + 3:]
        elif tick >= 0:
            out.append((False, rest[:tick]))
            after = rest[tick + 1:]
            line_end = after.find("\n")
            line = after if line_end < 0 else after[:line_end]
            close = line.find("`")
            if close >= 0:
                end = tick + 1 + close + 1
                out.append((True, rest[tick:end]))
                rest = rest[end:]
            else:
                out.append((False, rest[tick:tick + 1]))
                rest = rest[tick + 1:]
        else:
            out.append((False, rest))
            rest = ""
    return out


def scrub_prose(text):
    dashed = (text.replace("\\u2014", "—").replace(" – ", "—")
              .replace(" — ", "—").replace(" —", "—").replace("— ", "—"))
    out = []
    i = 0
    while i < len(dashed):
        c = dashed[i]
        if c != "—":
            out.append(c)
            i += 1
            continue
        before = next((ch for ch in reversed(out) if not ch.isspace()), None)
        nxt = dashed[i + 1] if i + 1 < len(dashed) else None
        at_line_start = True
        for ch in reversed(out):
            if ch == "\n":
                break
            if not ch.isspace():
                at_line_start = False
                break
        i += 1
        if nxt is None or nxt == "\n":
            continue
        if at_line_start:
            continue
        if before in (":", ",", ";", "(", ".", "!", "?"):
            out.append(" ")
            continue
        if nxt in (")", ",", ".", ";", ":", "!", "?"):
            continue
        while out and out[-1] == " ":
            out.pop()
        out.append(", ")
    return "".join(out)


def scrub(text):
    if "—" not in text and " – " not in text and "\\u2014" not in text:
        return text
    return "".join(part if code else scrub_prose(part) for code, part in parts(text))


def walk(value):
    if isinstance(value, str):
        return scrub(value)
    if isinstance(value, list):
        return [walk(v) for v in value]
    if isinstance(value, dict):
        return {k: walk(v) for k, v in value.items()}
    return value


changed = 0
for name in sys.argv[1:]:
    path = Path(name)
    text = path.read_text(encoding="utf-8")
    if path.suffix == ".json":
        data = json.loads(text)
        new = json.dumps(walk(data), indent=2, ensure_ascii=False) + "\n"
    else:
        new = scrub(text)
    if new != text:
        path.write_text(new, encoding="utf-8")
        changed += 1
        print(f"scrubbed {path}")
print(f"{changed} file(s) changed")
