# External evaluation grammars

These fixtures exercise parser-table construction only; Gazelle lexer
definitions are intentionally absent.

- `jq.gzl` is mechanically normalized from jq's native Bison grammar,
  `src/parser.y`, pinned at jq revision
  `2d410d6d86be7f685ad28e5cffac0248aa47664c`. Semantic actions and static
  precedence declarations are deliberately stripped so Gazelle and Bison are
  given the same bare grammar. License: MIT.
- `sqlite.gzl` is mechanically normalized from `sql/sqlite/SQLiteParser.g4`,
  pinned at grammars-v4 revision
  `e756f2a2ee5565a9300666f100ba6acd874664f7` and maintained by Bart
  Kiers and contributors. ANTLR labels and nongreedy annotations do not alter
  the context-free grammar and are removed. Gazelle has no complemented token
  set, so the grammar's two negated sets are each represented by an explicit
  wildcard-class terminal. It is an extended stress fixture because canonical
  construction produces tens of thousands of states. License: MIT.

Regenerate the fixtures with `scripts/import_bison_xml.py` and
`scripts/import_antlr_parser.py`; the exact upstream URLs and revisions are
embedded in each generated file. These normalized grammars are evaluation
inputs, not claims that Gazelle currently ships complete jq or SQLite front
ends.

## MIT notice for the jq grammar

Copyright (C) 2012 Stephen Dolan

Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files (the "Software"), to deal in
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
of the Software, and to permit persons to whom the Software is furnished to do
so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## MIT notice for the SQLite grammar

Copyright (c) 2014 Bart Kiers

Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files (the “Software”), to deal in
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
of the Software, and to permit persons to whom the Software is furnished to do
so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
