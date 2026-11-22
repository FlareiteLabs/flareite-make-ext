//! What a Makefile defines, read line by line as make reads it: rules
//! (their targets, prerequisites, and recipes), variables, `define`s, and
//! `include`s; and recipe lines indented with spaces, which make rejects
//! ("missing separator").

/// A variable a Makefile sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
  pub name: String,
  /// 0-based, where the assignment starts.
  pub line: u32,
  /// The assignment as written, continuation lines joined.
  pub source: String,
  /// The comment lines just above it.
  pub doc: String,
}

/// A rule's target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
  pub name: String,
  /// 0-based, the rule's line.
  pub line: u32,
  /// The rule's line, continuation lines joined.
  pub rule: String,
  /// Its recipe's lines, without their tabs.
  pub recipe: Vec<String>,
  /// The comment lines just above the rule.
  pub doc: String,
}

/// A recipe line indented with spaces instead of a tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpacedRecipe {
  /// 0-based.
  pub line: u32,
  /// How many spaces (and tabs after them) start it, in bytes.
  pub indent: u32,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outline {
  pub variables: Vec<Variable>,
  pub targets: Vec<Target>,
  /// `.PHONY`'s prerequisites.
  pub phony: Vec<String>,
  /// What `include` lines name (`$(...)` left as written), with their lines.
  pub includes: Vec<(String, u32)>,
  pub spaced_recipes: Vec<SpacedRecipe>,
  /// Whether `.RECIPEPREFIX` is set, so recipes needn't start with a tab.
  pub recipe_prefix: bool,
}

/// Where a line's comment starts: a `#` not escaped with `\`.
pub fn comment_start(line: &str) -> Option<usize> {
  let bytes = line.as_bytes();
  (0..bytes.len()).find(|&i| bytes[i] == b'#' && (i == 0 || bytes[i - 1] != b'\\'))
}

/// Assignment operators, longest first.
const OPERATORS: &[&str] = &["::=", ":::=", ":=", "?=", "+=", "!=", "="];

/// The variable an assignment sets, if `code` is one: `NAME op value`, after
/// any `export`, `override`, or `private`.
pub fn assignment(code: &str) -> Option<&str> {
  let mut rest = code.trim_start();
  loop {
    let word = rest.split_whitespace().next()?;
    if matches!(word, "export" | "override" | "private" | "unexport") && rest.len() > word.len() {
      rest = rest[word.len()..].trim_start();
    } else {
      break;
    }
  }
  // The name runs to whitespace or an operator.
  let end = rest
    .find(|c: char| c.is_whitespace() || matches!(c, '=' | ':' | '?' | '+' | '!'))
    .unwrap_or(rest.len());
  let (name, after) = rest.split_at(end);
  if name.is_empty() || name.contains(['$', '(', ')', '%']) {
    return None;
  }
  let after = after.trim_start();
  OPERATORS
    .iter()
    .any(|operator| after.starts_with(operator))
    .then_some(name)
}

const DIRECTIVES: &[&str] = &[
  "ifeq", "ifneq", "ifdef", "ifndef", "else", "endif", "include", "-include", "sinclude", "define",
  "endef", "export", "unexport", "override", "undefine", "vpath", "private",
];

fn first_word(code: &str) -> &str {
  code.split_whitespace().next().unwrap_or("")
}

/// A rule's targets and prerequisites, if `code` is a rule line (with
/// `:`, `::`, or `&:` outside `$(...)`, and not an assignment).
fn rule(code: &str) -> Option<(Vec<String>, String)> {
  let mut depth = 0i32;
  let bytes = code.as_bytes();
  for i in 0..bytes.len() {
    match bytes[i] {
      b'(' | b'{' => depth += 1,
      b')' | b'}' => depth -= 1,
      b':' if depth == 0 => {
        // `:=` and `::=` are assignments, as is `a ::= b`.
        let after = &code[i..];
        if after.starts_with(":=") || after.starts_with("::=") || after.starts_with(":::=") {
          return None;
        }
        // A drive letter (C:\) isn't a rule's colon.
        if i == 1 && bytes[0].is_ascii_alphabetic() && matches!(bytes.get(2), Some(b'\\' | b'/')) {
          continue;
        }
        let targets: Vec<String> = code[..i]
          .trim_end_matches('&')
          .split_whitespace()
          .map(str::to_owned)
          .collect();
        if targets.is_empty() {
          return None;
        }
        let rest = code[i..].trim_start_matches(':');
        // Prerequisites, up to a recipe after `;`.
        let rest = rest.split(';').next().unwrap_or("");
        return Some((targets, rest.trim().to_owned()));
      }
      _ => {}
    }
  }
  None
}

/// Reads a Makefile.
pub fn outline(text: &str) -> Outline {
  let mut outline = Outline::default();
  let lines: Vec<&str> = text.lines().collect();
  let mut comments: Vec<String> = Vec::new();
  // The targets whose recipe the lines that follow may be.
  let mut in_recipe: Option<Vec<usize>> = None;
  let mut in_define = false;
  let mut index = 0;
  while index < lines.len() {
    let number = index as u32;
    let first = lines[index];
    // A logical line: continued while it ends with `\`.
    let mut logical = first.to_owned();
    while logical.ends_with('\\') && index + 1 < lines.len() {
      logical.pop();
      index += 1;
      logical.push(' ');
      logical.push_str(lines[index].trim_start());
    }
    index += 1;

    if in_define {
      if first_word(&logical) == "endef" {
        in_define = false;
      }
      continue;
    }

    // A recipe line: a tab first, while a rule's recipe goes on.
    if (first.starts_with('\t') || (outline.recipe_prefix && in_recipe.is_some()))
      && let Some(targets) = &in_recipe
    {
      let line = logical.trim_start_matches('\t').to_owned();
      for &target in targets {
        outline.targets[target].recipe.push(line.clone());
      }
      continue;
    }

    let code = match comment_start(&logical) {
      Some(i) => &logical[..i],
      None => &logical[..],
    };
    let trimmed = code.trim();
    if trimmed.is_empty() {
      match comment_start(&logical) {
        Some(i) if first.trim_start().starts_with('#') => {
          comments.push(logical[i + 1..].trim().to_owned());
        }
        _ => comments.clear(),
      }
      continue;
    }
    let doc = std::mem::take(&mut comments).join("\n");

    // Spaces where a recipe's tab should be: a line in a recipe's place
    // that isn't make's own (a directive, an assignment, or a rule).
    let indented = first.starts_with(' ');
    if indented
      && in_recipe.is_some()
      && !outline.recipe_prefix
      && !DIRECTIVES.contains(&first_word(trimmed))
      && assignment(trimmed).is_none()
      && rule(trimmed).is_none()
    {
      let indent = first.len() - first.trim_start_matches([' ', '\t']).len();
      outline.spaced_recipes.push(SpacedRecipe {
        line: number,
        indent: indent as u32,
      });
      continue;
    }

    let word = first_word(trimmed);
    if word == "define" {
      in_define = true;
      if let Some(name) = trimmed.split_whitespace().nth(1) {
        outline.variables.push(Variable {
          name: name.to_owned(),
          line: number,
          source: trimmed.to_owned(),
          doc,
        });
      }
      in_recipe = None;
      continue;
    }
    if matches!(word, "include" | "-include" | "sinclude") {
      for path in trimmed.split_whitespace().skip(1) {
        outline.includes.push((path.to_owned(), number));
      }
      in_recipe = None;
      continue;
    }
    if DIRECTIVES.contains(&word) && assignment(trimmed).is_none() {
      // Conditionals don't end a recipe.
      if !matches!(
        word,
        "ifeq" | "ifneq" | "ifdef" | "ifndef" | "else" | "endif"
      ) {
        in_recipe = None;
      }
      continue;
    }
    if let Some(name) = assignment(trimmed) {
      if name == ".RECIPEPREFIX" {
        outline.recipe_prefix = true;
      }
      outline.variables.push(Variable {
        name: name.to_owned(),
        line: number,
        source: trimmed.to_owned(),
        doc,
      });
      in_recipe = None;
      continue;
    }
    if let Some((targets, prerequisites)) = rule(trimmed) {
      // `target: VAR = value` sets a variable for the target: no recipe.
      if assignment(&prerequisites).is_some() {
        in_recipe = None;
        continue;
      }
      if targets.iter().any(|target| target == ".PHONY") {
        outline
          .phony
          .extend(prerequisites.split_whitespace().map(str::to_owned));
        in_recipe = None;
        continue;
      }
      let mut indices = Vec::new();
      // A recipe on the rule's line, after `;`.
      let inline: Option<String> = trimmed
        .split_once(';')
        .map(|(_, recipe)| recipe.trim().to_owned());
      for name in targets {
        indices.push(outline.targets.len());
        outline.targets.push(Target {
          name,
          line: number,
          rule: trimmed.to_owned(),
          recipe: inline.iter().cloned().collect(),
          doc: doc.clone(),
        });
      }
      in_recipe = Some(indices);
      continue;
    }
    in_recipe = None;
  }
  outline
}

impl Outline {
  /// The targets someone would run: not special (`.PHONY`), patterns
  /// (`%.o`), or variables; each name once, in order.
  pub fn runnable_targets(&self) -> Vec<&Target> {
    let mut seen = std::collections::HashSet::new();
    self
      .targets
      .iter()
      .filter(|target| {
        !target.name.starts_with('.')
          && !target.name.contains(['%', '$', '(', '*'])
          && seen.insert(target.name.as_str())
      })
      .collect()
  }

  /// The default goal: the first runnable target, unless `.DEFAULT_GOAL`
  /// says otherwise.
  pub fn default_goal(&self) -> Option<String> {
    if let Some(variable) = self
      .variables
      .iter()
      .rev()
      .find(|variable| variable.name == ".DEFAULT_GOAL")
      && let Some((_, value)) = variable.source.split_once('=')
    {
      let value = value.trim();
      if !value.is_empty() {
        return Some(value.to_owned());
      }
    }
    self
      .runnable_targets()
      .first()
      .map(|target| target.name.clone())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const MAKEFILE: &str = "\
# The compiler.
CC ?= gcc
CFLAGS := -Wall \\
  -O2
export PATH := $(PATH):bin
SOURCES = $(wildcard src/*.c)
include config.mk

.PHONY: all clean

# Builds everything.
all: app

app: $(SOURCES:.c=.o)
\t$(CC) -o $@ $^
ifdef DEBUG
\t@echo debug
endif

%.o: %.c
\t$(CC) $(CFLAGS) -c $< -o $@

clean:
    rm -f app *.o
debug: CFLAGS += -g
test: ; ./app --test

define HELP
Usage: make
endef
";

  #[test]
  fn reads_variables_targets_and_includes() {
    let outline = outline(MAKEFILE);
    let variables: Vec<_> = outline
      .variables
      .iter()
      .map(|v| (v.name.as_str(), v.line))
      .collect();
    assert_eq!(
      variables,
      [
        ("CC", 1),
        ("CFLAGS", 2),
        ("PATH", 4),
        ("SOURCES", 5),
        ("HELP", 27)
      ]
    );
    assert_eq!(outline.variables[0].doc, "The compiler.");
    assert_eq!(outline.variables[1].source, "CFLAGS := -Wall  -O2");
    assert_eq!(outline.includes, [("config.mk".to_owned(), 6)]);
    assert_eq!(outline.phony, ["all", "clean"]);
    let targets: Vec<_> = outline
      .targets
      .iter()
      .map(|t| (t.name.as_str(), t.line))
      .collect();
    assert_eq!(
      targets,
      [
        ("all", 11),
        ("app", 13),
        ("%.o", 19),
        ("clean", 22),
        ("test", 25)
      ]
    );
    let app = &outline.targets[1];
    assert_eq!(app.recipe, ["$(CC) -o $@ $^", "@echo debug"]);
    assert_eq!(outline.targets[0].doc, "Builds everything.");
    assert_eq!(outline.targets[4].recipe, ["./app --test"]);
    let runnable: Vec<_> = outline
      .runnable_targets()
      .iter()
      .map(|t| t.name.as_str())
      .collect();
    assert_eq!(runnable, ["all", "app", "clean", "test"]);
    assert_eq!(outline.default_goal().as_deref(), Some("all"));
  }

  #[test]
  fn finds_recipes_indented_with_spaces() {
    let outline = outline(MAKEFILE);
    assert_eq!(
      outline.spaced_recipes,
      [SpacedRecipe {
        line: 23,
        indent: 4
      }]
    );
    // Indented assignments and directives after a rule are make's own.
    let fine = outline_of("all:\n\techo hi\n  X = 1\n  ifdef Y\n  endif\n");
    assert!(fine.spaced_recipes.is_empty());
    // With .RECIPEPREFIX, recipes needn't start with a tab.
    let prefixed = outline_of(".RECIPEPREFIX = >\nall:\n    > echo hi\n");
    assert!(prefixed.spaced_recipes.is_empty());
  }

  fn outline_of(text: &str) -> Outline {
    outline(text)
  }

  #[test]
  fn tells_assignments_from_rules() {
    assert_eq!(assignment("CC = gcc"), Some("CC"));
    assert_eq!(assignment("override X+=1"), Some("X"));
    assert_eq!(assignment("all: app"), None);
    assert_eq!(assignment("$(X) = 1"), None);
    assert_eq!(
      rule("a b: c d ; e").map(|r| r.0),
      Some(vec!["a".into(), "b".into()])
    );
    assert_eq!(rule("X := 1"), None);
    assert_eq!(rule("C:\\x = 1"), None);
    assert_eq!(rule("$(OUT)/%.o: %.c").map(|r| r.1), Some("%.c".into()));
    assert_eq!(comment_start("A = \\# not # yes"), Some(11));
  }

  #[test]
  fn uses_the_default_goal_variable() {
    let outline = outline_of("a:\nb:\n.DEFAULT_GOAL := b\n");
    assert_eq!(outline.default_goal().as_deref(), Some("b"));
  }
}
