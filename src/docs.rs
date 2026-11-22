//! What GNU make's functions, automatic variables, and well-known variables
//! do, for hover and completion, after the GNU make manual.

/// Functions: name, how it's called, and what it does.
pub const FUNCTIONS: &[(&str, &str, &str)] = &[
  (
    "subst",
    "$(subst from,to,text)",
    "Replaces every `from` in `text` with `to`.",
  ),
  (
    "patsubst",
    "$(patsubst pattern,replacement,text)",
    "Replaces the words of `text` that match `pattern` (with `%` as a wildcard) with `replacement`, where `%` is what matched.",
  ),
  (
    "strip",
    "$(strip string)",
    "Removes leading and trailing whitespace, and makes each run of whitespace one space.",
  ),
  (
    "findstring",
    "$(findstring find,in)",
    "`find` if `in` contains it; otherwise empty.",
  ),
  (
    "filter",
    "$(filter pattern...,text)",
    "The words of `text` that match any of the patterns.",
  ),
  (
    "filter-out",
    "$(filter-out pattern...,text)",
    "The words of `text` that match none of the patterns.",
  ),
  (
    "sort",
    "$(sort list)",
    "The words of `list` sorted, with duplicates removed.",
  ),
  (
    "word",
    "$(word n,text)",
    "The `n`th word of `text` (from 1).",
  ),
  ("words", "$(words text)", "The number of words in `text`."),
  (
    "wordlist",
    "$(wordlist s,e,text)",
    "The words of `text` from the `s`th to the `e`th.",
  ),
  ("firstword", "$(firstword names...)", "The first word."),
  ("lastword", "$(lastword names...)", "The last word."),
  (
    "dir",
    "$(dir names...)",
    "The folder part of each file name, up to and including the last `/`.",
  ),
  (
    "notdir",
    "$(notdir names...)",
    "Each file name without its folder part.",
  ),
  (
    "suffix",
    "$(suffix names...)",
    "The suffix of each file name (`.c`).",
  ),
  (
    "basename",
    "$(basename names...)",
    "Each file name without its suffix.",
  ),
  (
    "addsuffix",
    "$(addsuffix suffix,names...)",
    "Appends `suffix` to each name.",
  ),
  (
    "addprefix",
    "$(addprefix prefix,names...)",
    "Prepends `prefix` to each name.",
  ),
  (
    "join",
    "$(join list1,list2)",
    "Joins the two lists word by word.",
  ),
  (
    "wildcard",
    "$(wildcard pattern...)",
    "The existing files whose names match the patterns.",
  ),
  (
    "realpath",
    "$(realpath names...)",
    "The canonical absolute name of each existing file.",
  ),
  (
    "abspath",
    "$(abspath names...)",
    "The absolute name of each file, without resolving links.",
  ),
  (
    "if",
    "$(if condition,then-part[,else-part])",
    "`then-part` if `condition` expands to something non-empty; otherwise `else-part`.",
  ),
  (
    "or",
    "$(or condition1[,condition2...])",
    "The first condition that expands to something non-empty.",
  ),
  (
    "and",
    "$(and condition1[,condition2...])",
    "The last condition if all expand to something non-empty; otherwise empty.",
  ),
  (
    "intcmp",
    "$(intcmp lhs,rhs[,lt-part[,eq-part[,gt-part]]])",
    "Compares two integers, expanding the part for the result (GNU make 4.4).",
  ),
  (
    "foreach",
    "$(foreach var,list,text)",
    "`text` expanded for each word of `list` in turn as `var`, joined with spaces.",
  ),
  (
    "let",
    "$(let var [var ...],[list],text)",
    "`text` with the variables bound to the words of `list` (GNU make 4.4).",
  ),
  (
    "file",
    "$(file op filename[,text])",
    "Writes (`>`), appends (`>>`), or reads (`<`) a file.",
  ),
  (
    "call",
    "$(call variable,param,...)",
    "Expands `variable` with `$(1)`, `$(2)`, ... as the parameters.",
  ),
  (
    "value",
    "$(value variable)",
    "The variable's value, unexpanded.",
  ),
  (
    "eval",
    "$(eval text)",
    "Reads `text` as makefile syntax: it can define rules and variables.",
  ),
  (
    "origin",
    "$(origin variable)",
    "Where a variable came from: `undefined`, `default`, `environment`, `file`, `command line`, `override`, or `automatic`.",
  ),
  (
    "flavor",
    "$(flavor variable)",
    "How a variable expands: `undefined`, `recursive`, or `simple`.",
  ),
  (
    "error",
    "$(error text...)",
    "Stops make with `text` as the error.",
  ),
  (
    "warning",
    "$(warning text...)",
    "Prints `text` as a warning, with the makefile's name and line.",
  ),
  ("info", "$(info text...)", "Prints `text`."),
  (
    "shell",
    "$(shell command)",
    "The output of running `command` in the shell, with newlines as spaces.",
  ),
  (
    "guile",
    "$(guile expression)",
    "The result of a GNU Guile expression, in a make built with Guile.",
  ),
];

/// Automatic variables, set in a rule's recipe.
pub const AUTOMATIC: &[(&str, &str)] = &[
  (
    "@",
    "The target's file name. In a rule with several targets, the one that made the recipe run.",
  ),
  (
    "%",
    "The member name, when the target is an archive member (`lib.a(member.o)`).",
  ),
  ("<", "The first prerequisite."),
  ("?", "The prerequisites newer than the target."),
  ("^", "All the prerequisites, each once."),
  ("+", "All the prerequisites, with duplicates, in order."),
  ("|", "The order-only prerequisites."),
  ("*", "The stem the pattern's `%` matched."),
];

/// Variables make sets or uses itself.
pub const VARIABLES: &[(&str, &str)] = &[
  ("CC", "The C compiler (`cc`)."),
  ("CXX", "The C++ compiler (`g++`)."),
  ("CPP", "The C preprocessor (`$(CC) -E`)."),
  ("AS", "The assembler (`as`)."),
  ("AR", "The archiver (`ar`)."),
  ("LD", "The linker (`ld`)."),
  ("RM", "Removes files (`rm -f`)."),
  ("CFLAGS", "Flags for the C compiler."),
  ("CXXFLAGS", "Flags for the C++ compiler."),
  (
    "CPPFLAGS",
    "Flags for the C preprocessor (and the compilers that use it).",
  ),
  ("ASFLAGS", "Flags for the assembler."),
  ("LDFLAGS", "Flags for the compilers when they link (`-L`)."),
  ("LDLIBS", "Libraries to link with (`-lm`)."),
  ("ARFLAGS", "Flags for the archiver (`rv`)."),
  (
    "MAKE",
    "The make program running, for recursive makes: `$(MAKE) -C dir`.",
  ),
  (
    "MAKEFLAGS",
    "The flags make was given, passed to recursive makes.",
  ),
  ("MAKECMDGOALS", "The targets given on the command line."),
  ("MAKEFILE_LIST", "The makefiles read so far, in order."),
  ("CURDIR", "The folder make is working in."),
  ("SHELL", "The shell recipes run in (`/bin/sh`)."),
  ("VPATH", "Folders to look for prerequisites in."),
  (
    ".DEFAULT_GOAL",
    "The target `make` builds when given none: the first rule's, unless set.",
  ),
  (
    ".RECIPEPREFIX",
    "The character recipe lines start with: a tab, unless set.",
  ),
  (".SHELLFLAGS", "The flags given to the shell (`-c`)."),
  (
    ".ONESHELL",
    "As a target: run each recipe in one shell, not a shell per line.",
  ),
];

pub fn function(name: &str) -> Option<&'static (&'static str, &'static str, &'static str)> {
  FUNCTIONS.iter().find(|(function, _, _)| *function == name)
}

pub fn automatic(name: &str) -> Option<&'static str> {
  AUTOMATIC
    .iter()
    .find(|(variable, _)| *variable == name)
    .map(|(_, text)| *text)
}

pub fn variable(name: &str) -> Option<&'static str> {
  VARIABLES
    .iter()
    .find(|(variable, _)| *variable == name)
    .map(|(_, text)| *text)
}
