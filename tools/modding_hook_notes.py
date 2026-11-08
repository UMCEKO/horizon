#!/usr/bin/env python3
"""mdBook preprocessor: render catalog notes without writing a generated source page."""
import json
from pathlib import Path
import subprocess
import sys

PAGE = "reference/hook-notes.md"


def expand(book, content, title, path):
    """Fill the draft chapter from SUMMARY, retaining its published URL and position."""
    found = []

    def visit(items):
        for item in items:
            if "Chapter" not in item:
                continue
            chapter = item["Chapter"]
            if chapter["name"] == title and chapter["path"] is None:
                chapter.update(content=content, path=path, source_path=None)
                found.append(chapter)
            visit(chapter["sub_items"])

    visit(book["sections"])
    assert len(found) == 1, f"SUMMARY must contain one draft [{title}]() chapter"
    return book


def preprocess(context, book):
    root = Path(context["root"]).resolve().parents[1]
    content = subprocess.run(
        ["cargo", "run", "--quiet", "-p", "xtask", "--", "mod-api", "notes"],
        cwd=root, check=True, stdout=subprocess.PIPE, text=True,
    ).stdout
    return expand(book, content, "Hook notes", PAGE)


if __name__ == "__main__" and sys.argv[1:2] != ["supports"]:
    print(json.dumps(preprocess(*json.load(sys.stdin))))
