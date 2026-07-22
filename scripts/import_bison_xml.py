#!/usr/bin/env python3
"""Convert a Bison XML grammar report into a Gazelle evaluation fixture.

Run Bison with --xml first. The report contains the grammar after Bison has
expanded mid-rule actions, which lets this importer discard semantic C code
without trying to parse it. Static precedence metadata is intentionally not
copied: the evaluation feeds the resulting same bare grammar back to Bison.
"""

from __future__ import annotations

import argparse
import ast
import pathlib
import re
import sys
import xml.etree.ElementTree as ET

from import_antlr_parser import literal_name


def safe_name(name: str, terminal: bool) -> str:
    if ((name.startswith("'") and name.endswith("'")) or
            (name.startswith('"') and name.endswith('"'))):
        return literal_name(ast.literal_eval(name))
    if name == "error":
        return "BISON_ERROR"
    if name.startswith("$@"):
        return "__bison_midrule_" + name[2:]
    if name.startswith("$"):
        raise ValueError(f"unsupported Bison special symbol {name!r}")
    normalized = re.sub(r"[^A-Za-z0-9_]", "_", name)
    if not normalized or normalized[0].isdigit():
        normalized = ("TOK_" if terminal else "rule_") + normalized
    return normalized


def render(xml_source: str, source_url: str, revision: str,
           license_id: str) -> str:
    root = ET.fromstring(xml_source)
    grammar = root.find("grammar")
    if grammar is None:
        raise ValueError("Bison XML report has no grammar")

    terminal_names = {
        element.attrib["name"]
        for element in grammar.findall("./terminals/terminal")
        if element.attrib["name"] != "$end"
    }
    nonterminal_names = {
        element.attrib["name"]
        for element in grammar.findall("./nonterminals/nonterminal")
        if element.attrib["name"] != "$accept"
    }
    mapping = {name: safe_name(name, True) for name in terminal_names}
    mapping.update({name: safe_name(name, False) for name in nonterminal_names})
    if len(set(mapping.values())) != len(mapping):
        reverse: dict[str, list[str]] = {}
        for source, target in mapping.items():
            reverse.setdefault(target, []).append(source)
        collisions = {target: sources for target, sources in reverse.items()
                      if len(sources) > 1}
        raise ValueError(f"symbol-name collision after normalization: {collisions}")

    rule_elements = grammar.findall("./rules/rule")
    if not rule_elements or rule_elements[0].findtext("lhs") != "$accept":
        raise ValueError("expected Bison augmented rule 0")
    start_symbol = rule_elements[0].findtext("./rhs/symbol")
    if start_symbol is None:
        raise ValueError("augmented rule has no start symbol")

    grouped: dict[str, list[list[str]]] = {}
    for rule in rule_elements[1:]:
        lhs = rule.findtext("lhs")
        if lhs is None or lhs == "$accept":
            continue
        rhs = [symbol.text or "" for symbol in rule.findall("./rhs/symbol")]
        grouped.setdefault(mapping[lhs], []).append([mapping[symbol] for symbol in rhs])

    lines = [
        f"// SPDX-License-Identifier: {license_id}",
        "// Mechanically normalized Bison grammar for evaluation only.",
        f"// Upstream: {source_url}",
        f"// Revision: {revision}",
        "// Semantic actions and static precedence declarations were stripped.",
        "", f"start {mapping[start_symbol]};", "terminals {",
    ]
    terminals = sorted(mapping[name] for name in terminal_names)
    for offset in range(0, len(terminals), 8):
        chunk = ", ".join(terminals[offset:offset + 8])
        suffix = "," if offset + 8 < len(terminals) else ""
        lines.append(f"    {chunk}{suffix}")
    lines.extend(["}", ""])

    for lhs, alternatives in grouped.items():
        rendered = []
        for index, rhs in enumerate(alternatives, 1):
            rendered.append(f"{' '.join(rhs) if rhs else '_'} => alt_{index}")
        lines.append(f"{lhs} = " + "\n    | ".join(rendered) + ";")
        lines.append("")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("xml", type=pathlib.Path)
    parser.add_argument("--source-url", required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--license", required=True)
    parser.add_argument("--output", type=pathlib.Path)
    args = parser.parse_args()
    try:
        output = render(
            args.xml.read_text(), args.source_url, args.revision, args.license
        )
        if args.output:
            args.output.write_text(output)
        else:
            sys.stdout.write(output)
    except (OSError, ET.ParseError, ValueError) as error:
        print(f"import_bison_xml.py: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
