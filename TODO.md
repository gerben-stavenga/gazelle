# Gazelle TODO

- Automatic grammar rewrites for conflict resolution: for unambiguous grammars
  (fake LR(1) conflicts), automatically rewrite the grammar to be LR(1) while
  keeping the original grammar as the user-facing AST and API. CPS-style inlining
  is one instance, but the mechanism should be more general — e.g. counting
  constraints (C11 declaration specifiers), interleaved lists, etc. The user
  writes what they mean, gazelle makes it work.

