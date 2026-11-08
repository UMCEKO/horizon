#!/usr/bin/env python3
"""mdBook preprocessor: discover concept sources, retain published aggregate chapters."""
import argparse
from collections import Counter
from html.parser import HTMLParser
from itertools import chain
import json
from pathlib import Path
import posixpath
import re
import sys
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "docs/modding/src"
GROUPS = ("concepts/trait-mods.md", "how-to/native-mods.md")
MARKER = "<!-- concept-pages -->"
LINK = re.compile(r"(\!?\[[^\n]*?\]\()([^\s)]+)(\))")
REFERENCE = re.compile(r"^( {0,3}\[[^\]\n]+\]:[ \t]*<?)([^\s>]+)(>?)", re.MULTILINE)
INCLUDE = re.compile(r"(\{\{#(?:rustdoc_)?include )([^ }]+)(\s*\}\})")
TITLE = re.compile(r"<!-- chapter: (.+) -->\n(?:<!-- separator: (\d+) -->\n)?\n")


def headings(text):
    """Yield ATX headings outside fenced examples, using mdBook's heading IDs."""
    fenced = False
    counts = Counter()
    for line in text.splitlines():
        if line.startswith(("```", "~~~")):
            fenced = not fenced
        if not fenced and re.match(r"^#{1,6} ", line):
            title = line.split(" ", 1)[1]
            slug = re.sub(r"[^\w\- ]", "", title.lower()).replace(" ", "-")
            n = counts[slug]
            counts[slug] += 1
            yield title, slug + (f"-{n}" if n else "")


def rewrite(text, link, include):
    for pattern in (LINK, REFERENCE):
        text = pattern.sub(lambda m: m[1] + link(m[2]) + m[3], text)
    return INCLUDE.sub(lambda m: m[1] + include(m[2]) + m[3], text)


def relocate(url, source, dest, owners=None):
    parsed = urlsplit(url)
    if parsed.scheme or parsed.netloc or url.startswith("/"):
        return url
    path = posixpath.normpath(posixpath.join(posixpath.dirname(source), parsed.path)) if parsed.path else source
    anchor = parsed.fragment
    if owners and (path, anchor) in owners:
        path = owners[path, anchor]
    relative = "" if path == dest and anchor else posixpath.relpath(path, posixpath.dirname(dest))
    return relative + ("?" + parsed.query if parsed.query else "") + ("#" + anchor if anchor else "")


def relocate_include(value, source, dest):
    path, sep, anchor = value.partition(":")
    return relocate(path, source, dest) + sep + anchor


def fragments(src, group):
    paths = sorted((src / group.removesuffix(".md")).glob("*.md"))
    assert paths, f"{group}: no concept sources"
    result = []
    for path in paths:
        text = path.read_text()
        title = TITLE.match(text)
        assert title, f"{path}: missing chapter title"
        separator = int(title[2]) if title[2] is not None else 1
        result.append((path.relative_to(src).as_posix(), title[1], text[title.end():] + "\n" * separator))
    return result


def aggregate(src, group):
    parts = fragments(src, group)
    owners = {(path, anchor): g for g in GROUPS for path, _, text in fragments(src, g)
              for _, anchor in headings(text)}
    content, marker, suffix = (src / group).read_text().partition(MARKER)
    assert marker and not suffix.strip() and re.fullmatch(r"# [^\n]+\n\n", content), (
        f"{group}: aggregate must contain only its title and discovery marker")
    for path, _, text in parts:
        content += rewrite(text, lambda u: relocate(u, path, group, owners),
                           lambda u: relocate_include(u, path, group))
    legacy = (src / group.removesuffix(".md") / "legacy-anchors.txt").read_text().splitlines()
    missing = set(legacy) - {anchor for _, anchor in headings(content)}
    assert not missing, f"{group}: published anchors lost: {sorted(missing)}"
    return content


def validate(text, path, src, contents):
    for match in chain(LINK.finditer(text), REFERENCE.finditer(text)):
        url = urlsplit(match[2])
        if url.scheme or url.netloc:
            continue
        assert not unquote(url.path).startswith("/"), f"{path}: root-relative link {match[2]}"
        target = (src / path).parent / unquote(url.path) if url.path else src / path
        target = target.resolve()
        assert target in contents or target.is_file(), f"{path}: missing link target {match[2]}"
        if url.fragment and target.suffix == ".md":
            body = contents[target] if target in contents else target.read_text()
            assert unquote(url.fragment) in {a for _, a in headings(body)}, f"{path}: missing anchor {match[2]}"
    for match in INCLUDE.finditer(text):
        name, _, anchor = match[2].partition(":")
        target = (src / path).parent / name
        assert target.is_file(), f"{path}: missing include {name}"
        if anchor and not anchor[0].isdigit():
            assert f"ANCHOR: {anchor}" in target.read_text(), f"{path}: missing include anchor {anchor}"


def expand(book, src):
    contents = {}

    def collect(items):
        for item in items:
            if "Chapter" in item:
                chapter = item["Chapter"]
                if chapter["path"]:
                    contents[(src / chapter["path"]).resolve()] = chapter["content"]
                collect(chapter["sub_items"])

    collect(book["sections"])
    contents.update({(src / g).resolve(): aggregate(src, g) for g in GROUPS})
    seen = set()

    def visit(items):
        for item in items:
            if "Chapter" not in item:
                continue
            chapter = item["Chapter"]
            group = chapter["path"]
            if group not in GROUPS:
                visit(chapter["sub_items"])
                continue
            seen.add(group)
            assert MARKER in chapter["content"], f"{group}: missing discovery marker"
            chapter["content"] = contents[(src / group).resolve()]
            children = []
            for i, (path, title, text) in enumerate(fragments(src, group)):
                if not text.startswith("#"):
                    text = f"# {title}\n\n" + text
                validate(text, path, src, contents)
                children.append({"Chapter": dict(name=title.replace("`", ""), content=text,
                    path=path, source_path=path, number=(chapter["number"] or []) + [i + 1],
                    parent_names=chapter["parent_names"] + [chapter["name"]], sub_items=[])})
            validate(chapter["content"], group, src, contents)
            chapter["sub_items"] = children
    visit(book["sections"])
    assert seen == set(GROUPS), "SUMMARY must list both aggregate chapters"
    return book


class HtmlLinks(HTMLParser):
    def __init__(self, text):
        super().__init__()
        self.ids, self.links = set(), []
        self.feed(text)

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if "id" in attrs:
            self.ids.add(attrs["id"])
        if tag == "a" and "href" in attrs:
            self.links.append(attrs["href"])


def check_html(directory, src=SRC):
    pages = [p for g in GROUPS for p in [g, *(p for p, _, _ in fragments(src, g))]]
    pages.extend(["reference/hook-notes.md", "reference/neoforge-counterparts.md"])
    for page in pages:
        path = directory / Path(page).with_suffix(".html")
        html = HtmlLinks(path.read_text())
        for link in html.links:
            url = urlsplit(link)
            if url.scheme or url.netloc:
                continue
            assert not unquote(url.path).startswith("/"), f"{page}: root-relative link {link}"
            target = (path.parent / unquote(url.path)).resolve() if url.path else path
            # Existing links outside the book refer to repository source, not book pages.
            if not target.is_relative_to(directory.resolve()):
                continue
            assert target.is_file(), f"{page}: missing rendered link {link}"
            if url.fragment:
                assert unquote(url.fragment) in HtmlLinks(target.read_text()).ids, f"{page}: missing rendered anchor {link}"
    print(f"Rendered relative links and anchors: valid in {len(pages)} chapters")


def main():
    if sys.argv[1:2] == ["supports"]:
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--html", type=Path)
    args = parser.parse_args()
    if args.html:
        check_html(args.html.resolve())
    else:
        context, book = json.load(sys.stdin)
        print(json.dumps(expand(book, Path(context["root"]) / context["config"]["book"]["src"])))


if __name__ == "__main__":
    main()
