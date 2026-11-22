; Makefiles, after the grammar's own highlights.scm
; (tree-sitter-grammars/tree-sitter-make 1.1.1), with Flareite's captures.
; Narrower captures win, and earlier patterns win for the same node.

(comment) @comment

; Targets: special ones (.PHONY) as constants, the rest as functions,
; since they're what `make <target>` runs.
(targets
  (word) @constant.builtin
  (#match? @constant.builtin "^\\.(PHONY|SUFFIXES|DEFAULT|PRECIOUS|INTERMEDIATE|NOTINTERMEDIATE|SECONDARY|SECONDEXPANSION|DELETE_ON_ERROR|IGNORE|LOW_RESOLUTION_TIME|SILENT|EXPORT_ALL_VARIABLES|NOTPARALLEL|ONESHELL|POSIX|WAIT)$"))
(targets
  (word) @function)

; Variables: make's own, then the rest.
(variable_assignment
  name: (word) @variable.builtin
  (#match? @variable.builtin "^(AR|AS|CC|CXX|CPP|FC|LEX|YACC|RM|MAKE|MAKEFLAGS|SHELL|ARFLAGS|ASFLAGS|CFLAGS|CXXFLAGS|CPPFLAGS|FFLAGS|LDFLAGS|LDLIBS|LFLAGS|YFLAGS|VPATH|MAKEFILE_LIST|\\.DEFAULT_GOAL|\\.RECIPEPREFIX|\\.SHELLFLAGS|\\.EXTRA_PREREQS)$"))
(variable_assignment
  name: (word) @property)
(define_directive
  name: (word) @property)
(variable_reference
  (word) @variable.builtin
  (#match? @variable.builtin "^(AR|AS|CC|CXX|CPP|FC|LEX|YACC|RM|MAKE|MAKEFLAGS|SHELL|ARFLAGS|ASFLAGS|CFLAGS|CXXFLAGS|CPPFLAGS|FFLAGS|LDFLAGS|LDLIBS|LFLAGS|YFLAGS|VPATH|MAKEFILE_LIST|CURDIR|MAKECMDGOALS)$"))
(variable_reference
  (word) @property)
(automatic_variable) @variable.builtin

; Patterns and wildcards.
((word) @string.special
  (#match? @string.special "[%*?]"))

[
  "ifeq"
  "ifneq"
  "ifdef"
  "ifndef"
  "else"
  "endif"
  "if"
  "or"
  "and"
  "foreach"
  "define"
  "endef"
  "vpath"
  "undefine"
  "export"
  "unexport"
  "override"
  "private"
  "include"
  "sinclude"
  "-include"
] @keyword

[
  "subst"
  "patsubst"
  "strip"
  "findstring"
  "filter"
  "filter-out"
  "sort"
  "word"
  "words"
  "wordlist"
  "firstword"
  "lastword"
  "dir"
  "notdir"
  "suffix"
  "basename"
  "addsuffix"
  "addprefix"
  "join"
  "wildcard"
  "realpath"
  "abspath"
  "call"
  "eval"
  "file"
  "value"
  "shell"
  "error"
  "warning"
  "info"
] @function.builtin

[
  (text)
  (string)
  (raw_text)
] @string

[
  "="
  ":="
  "::="
  "?="
  "+="
  "!="
  "@"
  "-"
  "+"
] @operator

[
  "("
  ")"
  "{"
  "}"
] @punctuation.bracket

[
  ":"
  "&:"
  "::"
  "|"
  ";"
  ","
] @punctuation.delimiter

[
  "$"
  "$$"
] @punctuation.special
