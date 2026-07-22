#!/usr/bin/env python3
"""Convert an action-free ANTLR parser grammar to a Gazelle grammar fixture.

This intentionally targets the small ANTLR parser-grammar subset used by the
evaluation corpus: rules, alternatives, groups, and ?, *, + EBNF operators.
Lexer rules are not imported. Literal terminals receive stable symbolic names.
Semantic actions and predicates are rejected by default; --strip-code exists
for corpus experiments whose provenance notes explicitly record that change.
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys


PUNCTUATION_NAMES = {
    "(": "LPAREN",
    ")": "RPAREN",
    "[": "LBRACK",
    "]": "RBRACK",
    "{": "LBRACE",
    "}": "RBRACE",
    ";": "SEMI",
    ",": "COMMA",
    ".": "DOT",
    "...": "ELLIPSIS",
    ":": "COLON",
    "::": "COLONCOLON",
    "?": "QUESTION",
    "@": "AT",
    "=": "EQ",
    "==": "EQEQ",
    "!=": "NE",
    "~=": "TILDEEQ",
    "<": "LT",
    ">": "GT",
    "<=": "LE",
    ">=": "GE",
    "+": "PLUS",
    "-": "MINUS",
    "*": "STAR",
    "/": "SLASH",
    "//": "SLASHSLASH",
    "%": "PERCENT",
    "^": "CARET",
    "&": "AMP",
    "|": "PIPE",
    "~": "TILDE",
    "!": "BANG",
    "&&": "AMPAMP",
    "||": "PIPEPIPE",
    "++": "PLUSPLUS",
    "--": "MINUSMINUS",
    "<<": "LTLT",
    ">>": "GTGT",
    "->": "ARROW",
    "=>": "FATARROW",
    "+=": "PLUSEQ",
    "-=": "MINUSEQ",
    "*=": "STAREQ",
    "/=": "SLASHEQ",
    "%=": "PERCENTEQ",
    "&=": "AMPEQ",
    "|=": "PIPEEQ",
    "^=": "CARETEQ",
    "<<=": "LTLTEQ",
    ">>=": "GTGTEQ",
}


class ConversionError(Exception):
    pass


def strip_comments(source: str) -> str:
    result: list[str] = []
    index = 0
    quote = None
    escaped = False
    while index < len(source):
        if quote is not None:
            char = source[index]
            result.append(char)
            index += 1
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == quote:
                quote = None
            continue
        if source[index] in "'\"":
            quote = source[index]
            result.append(source[index])
            index += 1
        elif source.startswith("//", index):
            newline = source.find("\n", index + 2)
            index = len(source) if newline < 0 else newline
        elif source.startswith("/*", index):
            end = source.find("*/", index + 2)
            if end < 0:
                raise ConversionError("unclosed block comment")
            result.append("\n" * source[index:end + 2].count("\n"))
            index = end + 2
        else:
            result.append(source[index])
            index += 1
    return "".join(result)


def skip_balanced(source: str, start: int, opening: str, closing: str) -> int:
    depth = 0
    quote = None
    escaped = False
    for index in range(start, len(source)):
        char = source[index]
        if quote is not None:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == quote:
                quote = None
            continue
        if char in "'\"":
            quote = char
        elif char == opening:
            depth += 1
        elif char == closing:
            depth -= 1
            if depth == 0:
                return index + 1
    raise ConversionError(f"unclosed {opening!r} block")


def remove_preamble_and_code(source: str, strip_code: bool) -> str:
    match = re.search(r"\bparser\s+grammar\s+[A-Za-z_][A-Za-z0-9_]*\s*;", source)
    if not match:
        raise ConversionError("expected an ANTLR 'parser grammar Name;' declaration")
    source = source[match.end():].lstrip()
    if re.match(r"options\b", source):
        brace = source.find("{")
        if brace < 0:
            raise ConversionError("malformed options block")
        source = source[skip_balanced(source, brace, "{", "}"):]

    result: list[str] = []
    index = 0
    while index < len(source):
        if source[index] == "'":
            end = index + 1
            escaped = False
            while end < len(source):
                char = source[end]
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == "'":
                    end += 1
                    break
                end += 1
            else:
                raise ConversionError("unclosed string literal")
            result.append(source[index:end])
            index = end
            continue
        if source[index] == "{":
            end = skip_balanced(source, index, "{", "}")
            if not strip_code:
                snippet = source[index:end].replace("\n", " ")[:80]
                raise ConversionError(f"semantic code/predicate requires --strip-code: {snippet}")
            index = end
            if index < len(source) and source[index] == "?":
                index += 1
            continue
        if source[index] == "<":
            # ANTLR alternative metadata such as <assoc=right>.
            end = source.find(">", index + 1)
            if end < 0:
                raise ConversionError("unclosed ANTLR alternative metadata")
            if not strip_code:
                raise ConversionError("ANTLR alternative metadata requires --strip-code")
            index = end + 1
            continue
        result.append(source[index])
        index += 1
    return "".join(result)


TOKEN_RE = re.compile(
    r"\s+|"
    r"'(?:\\.|[^'\\])*'|"
    r"[A-Za-z_][A-Za-z0-9_]*|"
    r"\+=|=>|::|"
    r"[:;|()?*+=,]"
)


def tokenize(source: str) -> list[str]:
    tokens: list[str] = []
    index = 0
    while index < len(source):
        match = TOKEN_RE.match(source, index)
        if not match:
            snippet = source[index:index + 40].replace("\n", " ")
            raise ConversionError(f"unsupported ANTLR syntax near {snippet!r}")
        token = match.group(0)
        index = match.end()
        if not token.isspace():
            tokens.append(token)
    return tokens


def decode_literal(token: str) -> str:
    body = token[1:-1]
    return bytes(body, "utf-8").decode("unicode_escape")


def literal_name(value: str) -> str:
    if value in PUNCTUATION_NAMES:
        return f"TOK_{PUNCTUATION_NAMES[value]}"
    if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", value):
        return "KW_" + value.upper()
    encoded = "_".join(f"{byte:02X}" for byte in value.encode("utf-8"))
    return "LIT_" + encoded


def split_rules(tokens: list[str]) -> list[tuple[str, list[str]]]:
    rules: list[tuple[str, list[str]]] = []
    index = 0
    while index < len(tokens):
        name = tokens[index]
        if not re.fullmatch(r"[a-z_][A-Za-z0-9_]*", name):
            raise ConversionError(f"expected parser rule, found {name!r}")
        index += 1
        if index >= len(tokens) or tokens[index] != ":":
            raise ConversionError(f"expected ':' after rule {name}")
        index += 1
        depth = 0
        body: list[str] = []
        while index < len(tokens):
            token = tokens[index]
            index += 1
            if token == "(":
                depth += 1
            elif token == ")":
                depth -= 1
            elif token == ";" and depth == 0:
                break
            body.append(token)
        else:
            raise ConversionError(f"unterminated rule {name}")
        rules.append((name, body))
    return rules


def normalize_body(tokens: list[str], terminals: set[str]) -> list[str]:
    result: list[str] = []
    index = 0
    while index < len(tokens):
        token = tokens[index]
        # Discard ANTLR labels: name=atom and name+=atom.
        if (index + 1 < len(tokens) and
                re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", token) and
                tokens[index + 1] in {"=", "+="}):
            index += 2
            continue
        index += 1
        if token == "EOF":
            continue
        if token == "?" and result and result[-1] in {"*", "+", "?"}:
            # ANTLR nongreedy suffix; greediness does not alter the CFG.
            continue
        if token.startswith("'"):
            name = literal_name(decode_literal(token))
            terminals.add(name)
            result.append(name)
        else:
            if re.fullmatch(r"[A-Z][A-Za-z0-9_]*", token):
                terminals.add(token)
            result.append(token)
    return result


def split_alternatives(tokens: list[str]) -> list[list[str]]:
    alternatives: list[list[str]] = [[]]
    depth = 0
    for token in tokens:
        if token == "(":
            depth += 1
        elif token == ")":
            depth -= 1
        if token == "|" and depth == 0:
            alternatives.append([])
        else:
            alternatives[-1].append(token)
    return alternatives


def lower_groups(owner: str, tokens: list[str], counter: list[int],
                 generated: list[tuple[str, list[list[str]]]]) -> list[str]:
    """Replace ANTLR sequence/alternative groups with synthetic rules."""
    result: list[str] = []
    index = 0
    while index < len(tokens):
        token = tokens[index]
        if token != "(":
            result.append(token)
            index += 1
            continue
        depth = 1
        end = index + 1
        while end < len(tokens) and depth:
            if tokens[end] == "(":
                depth += 1
            elif tokens[end] == ")":
                depth -= 1
            end += 1
        if depth:
            raise ConversionError(f"unclosed group in rule {owner}")
        inner = lower_groups(owner, tokens[index + 1:end - 1], counter, generated)
        counter[0] += 1
        synthetic_name = f"__antlr_{owner}_group_{counter[0]}"
        generated.append((synthetic_name, split_alternatives(inner)))
        result.append(synthetic_name)
        index = end
    return result


def render(source: str, source_url: str, revision: str, license_id: str,
           strip_code: bool, start_rule: str | None) -> str:
    cleaned = remove_preamble_and_code(strip_comments(source), strip_code)
    negated_sets = 0

    def replace_negated_set(match: re.Match[str]) -> str:
        nonlocal negated_sets
        negated_sets += 1
        return f"ANTLR_NOT_SET_{negated_sets}"

    # Gazelle has no token-set complement operator. Preserve each occurrence
    # as one explicit wildcard-class terminal; provenance notes make this
    # normalization visible instead of pretending it is a literal translation.
    cleaned = re.sub(r"~\([^()]*\)", replace_negated_set, cleaned)
    rules = split_rules(tokenize(cleaned))
    if not rules:
        raise ConversionError("grammar contains no parser rules")
    rule_names = {name for name, _ in rules}
    start_rule = start_rule or rules[0][0]
    if start_rule not in rule_names:
        raise ConversionError(f"unknown start rule {start_rule!r}")

    terminals: set[str] = set()
    normalized: list[tuple[str, list[list[str]]]] = []
    generated: list[tuple[str, list[list[str]]]] = []
    group_counter = [0]
    for name, body in rules:
        body = normalize_body(body, terminals)
        body = lower_groups(name, body, group_counter, generated)
        normalized.append((name, split_alternatives(body)))
    normalized.extend(generated)

    lines = [
        f"// SPDX-License-Identifier: {license_id}",
        "// Mechanically translated parser grammar for evaluation only.",
        f"// Upstream: {source_url}",
        f"// Revision: {revision}",
        "// Lexer rules and EOF markers are outside this table comparison.",
    ]
    if strip_code:
        lines.append("// Semantic actions, predicates, and ANTLR metadata were stripped.")
    if negated_sets:
        lines.append(
            f"// {negated_sets} negated token set(s) were collapsed to wildcard-class terminals."
        )
    lines.extend(["", f"start {start_rule};", "terminals {"])
    terminal_list = sorted(terminals)
    for offset in range(0, len(terminal_list), 8):
        chunk = ", ".join(terminal_list[offset:offset + 8])
        suffix = "," if offset + 8 < len(terminal_list) else ""
        lines.append(f"    {chunk}{suffix}")
    lines.extend(["}", ""])

    for name, alternatives in normalized:
        rendered_alts = []
        for alt_index, alternative in enumerate(alternatives):
            rhs = " ".join(alternative) if alternative else "_"
            rendered_alts.append(f"{rhs} => alt_{alt_index + 1}")
        lines.append(f"{name} = " + "\n    | ".join(rendered_alts) + ";")
        lines.append("")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=pathlib.Path)
    parser.add_argument("--source-url", required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--license", required=True)
    parser.add_argument("--strip-code", action="store_true")
    parser.add_argument("--start")
    parser.add_argument("--output", type=pathlib.Path)
    args = parser.parse_args()
    try:
        output = render(
            args.source.read_text(), args.source_url, args.revision,
            args.license, args.strip_code, args.start,
        )
    except (OSError, ConversionError) as error:
        print(f"import_antlr_parser.py: {error}", file=sys.stderr)
        return 1
    if args.output:
        args.output.write_text(output)
    else:
        sys.stdout.write(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
