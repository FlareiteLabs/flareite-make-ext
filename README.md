# Makefile

Makefiles for Flareite: `Makefile`, `makefile`, `GNUmakefile`, and `.mk` files.

- **Highlighting** from a Tree-sitter grammar
  ([tree-sitter-grammars/tree-sitter-make](https://github.com/tree-sitter-grammars/tree-sitter-make)):
  targets, variables, make's functions and automatic variables, directives, and comments. Rules,
  conditionals, and `define`s fold.
- **Tabs.** Recipe lines must start with a tab, so Tab inserts one in makefiles whatever your
  indentation settings say (a `.editorconfig` still decides). A recipe line that starts with
  spaces, which make stops at ("missing separator"), is a problem, with a fix that puts a tab
  there (Ctrl+.).
- **The Make view** (in the activity bar) lists the folder's makefiles and their targets, the
  default goal first. Click a target to go to its rule. Its buttons **run it in a terminal**
  (`make -f Makefile <target>`) or **build it** in the background.
- **Building** follows make's output and puts the errors and warnings it prints in Problems, in
  any file, open or not: gcc, clang, NASM, MSVC-style tools, and make's own. The status bar shows
  the build while it runs and how it ended; click it to build again. **Make: Build Target**,
  **Build Default Goal**, **Build Again**, **Stop Build**, and **Clear Build Problems** are
  commands too.
- **Completion and hover.** After `$(` or `${`: the file's variables (and those of the files it
  includes), make's own (`CC`, `CFLAGS`, `MAKE`, ...), and its functions (`patsubst`,
  `wildcard`, ...). After a rule's colon: targets. Hovering shows a variable's definitions, what
  a function or an automatic variable (`$@`, `$<`, `$^`) does, and a target's rule and recipe.

## Settings

| Setting     | Default | What it does                                         |
| ----------- | ------- | ---------------------------------------------------- |
| `arguments` | (none)  | More arguments for make, such as `-j8` or `DEBUG=1`. |

make runs only in folders you trust. If GNU make isn't on the PATH as `make`, set where it is in
`settings.toml` (MSYS2's `mingw32-make` works too):

```toml
[extensions.programs]
"flareite.makefile.make" = "C:/msys64/ucrt64/bin/mingw32-make.exe"
```
