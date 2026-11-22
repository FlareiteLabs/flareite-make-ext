//! Problems in what a build printed. Compilers and assemblers that make
//! runs report them as `file:line:column: severity: message` (gcc, clang,
//! ld, and many more) or `file:line: severity: message` (NASM with `-X
//! gnu`, make itself); MSVC-style tools as `file(line,column): severity
//! code: message`. Paths are relative to the folder make was in, which
//! `make[1]: Entering directory '...'` lines follow.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
  Error,
  Warning,
  Note,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
  /// As printed, joined to the folder make was in when it's relative.
  pub file: String,
  /// 1-based.
  pub line: u32,
  /// 1-based; 0 when none was given.
  pub column: u32,
  pub level: Level,
  pub message: String,
}

fn is_absolute(path: &str) -> bool {
  path.starts_with(['/', '\\']) || path.as_bytes().get(1) == Some(&b':')
}

pub fn join(folder: &str, path: &str) -> String {
  if is_absolute(path) || folder.is_empty() {
    return path.to_owned();
  }
  let windows = folder.as_bytes().get(1) == Some(&b':') || folder.starts_with("\\\\");
  let separator = if windows { '\\' } else { '/' };
  let path = path.trim_start_matches("./");
  let path = if windows {
    path.replace('/', "\\")
  } else {
    path.to_owned()
  };
  format!("{}{separator}{path}", folder.trim_end_matches(['/', '\\']))
}

fn level_of(text: &str) -> Option<(Level, &str)> {
  let text = text.trim_start();
  for (prefix, level) in [
    ("fatal error", Level::Error),
    ("error", Level::Error),
    ("warning", Level::Warning),
    ("note", Level::Note),
    ("fatal", Level::Error),
  ] {
    let Some(rest) = text.strip_prefix(prefix) else {
      continue;
    };
    // `error: message`
    if let Some(message) = rest.strip_prefix(':') {
      return Some((level, message.trim_start()));
    }
    // MSVC's `error C2065: message`
    if let Some((code, message)) = rest
      .strip_prefix(' ')
      .and_then(|rest| rest.split_once(": "))
      && !code.is_empty()
      && !code.contains(' ')
    {
      return Some((level, message));
    }
    return None;
  }
  None
}

/// `file:line[:column]: rest`, the file possibly holding a drive's colon.
fn gnu(line: &str) -> Option<(&str, u32, u32, &str)> {
  let mut search = 0;
  while let Some(found) = line[search..].find(':') {
    let colon = search + found;
    let after = &line[colon + 1..];
    let digits = after.len() - after.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits > 0 && colon > 0 {
      let number: u32 = after[..digits].parse().ok()?;
      let rest = &after[digits..];
      if let Some(rest) = rest.strip_prefix(": ") {
        return Some((&line[..colon], number, 0, rest));
      }
      if let Some(rest) = rest.strip_prefix(':') {
        let column_digits =
          rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if column_digits > 0
          && let Some(message) = rest[column_digits..].strip_prefix(": ")
        {
          let column = rest[..column_digits].parse().ok()?;
          return Some((&line[..colon], number, column, message));
        }
      }
    }
    search = colon + 1;
  }
  None
}

/// `file(line[,column]): rest`.
fn msvc(line: &str) -> Option<(&str, u32, u32, &str)> {
  let open = line.find('(')?;
  let close = open + line[open..].find("): ")?;
  let inside = &line[open + 1..close];
  let (number, column) = match inside.split_once(',') {
    Some((number, column)) => (number.trim().parse().ok()?, column.trim().parse().ok()?),
    None => (inside.trim().parse().ok()?, 0),
  };
  Some((line[..open].trim(), number, column, &line[close + 3..]))
}

/// Follows a build's output, line by line.
#[derive(Debug, Default)]
pub struct Reader {
  /// The folders make entered, innermost last, starting with where it ran.
  folders: Vec<String>,
  pub problems: Vec<Problem>,
}

impl Reader {
  pub fn new(folder: &str) -> Self {
    Reader {
      folders: vec![folder.to_owned()],
      problems: Vec::new(),
    }
  }

  fn folder(&self) -> &str {
    self.folders.last().map_or("", String::as_str)
  }

  pub fn line(&mut self, line: &str) {
    let line = line.trim_end();
    // make[1]: Entering directory '/x' and Leaving directory '/x'.
    if line.starts_with("make") {
      if let Some(at) = line.find(": Entering directory ") {
        let folder = line[at + 21..].trim_matches(['\'', '`', '"']);
        self.folders.push(folder.to_owned());
        return;
      }
      if line.contains(": Leaving directory ") {
        if self.folders.len() > 1 {
          self.folders.pop();
        }
        return;
      }
      // make: *** [Makefile:12: all] Error 1 repeats what was said.
      if line.contains("*** [") {
        return;
      }
    }
    // Makefile:3: *** missing separator.  Stop.
    if let Some((file, number, _, rest)) = gnu(line)
      && let Some(message) = rest.strip_prefix("*** ")
    {
      self.push(
        file,
        number,
        0,
        Level::Error,
        message.trim_end_matches(".  Stop."),
      );
      return;
    }
    let found = gnu(line).or_else(|| msvc(line));
    if let Some((file, number, column, rest)) = found
      && let Some((level, message)) = level_of(rest)
    {
      self.push(file, number, column, level, message);
    }
  }

  fn push(&mut self, file: &str, line: u32, column: u32, level: Level, message: &str) {
    let file = join(self.folder(), file);
    self.problems.push(Problem {
      file,
      line,
      column,
      level,
      message: message.trim().to_owned(),
    });
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn read(folder: &str, output: &str) -> Vec<Problem> {
    let mut reader = Reader::new(folder);
    for line in output.lines() {
      reader.line(line);
    }
    reader.problems
  }

  #[test]
  fn reads_gcc_nasm_msvc_and_make() {
    let problems = read(
      "/w",
      "gcc -c main.c\n\
       main.c:3:12: error: 'x' undeclared (first use in this function)\n\
       main.c:3:12: note: each undeclared identifier is reported only once\n\
       src/io.c:10:1: warning: unused variable 'y' [-Wunused-variable]\n\
       boot.asm:7: error: symbol `start' not defined\n\
       C:\\code\\a.c(4,5): error C2065: 'z': undeclared identifier\n\
       Makefile:9: *** missing separator.  Stop.\n\
       make: *** [Makefile:12: all] Error 1\n",
    );
    let summary: Vec<_> = problems
      .iter()
      .map(|p| (p.file.as_str(), p.line, p.column, p.level))
      .collect();
    assert_eq!(
      summary,
      [
        ("/w/main.c", 3, 12, Level::Error),
        ("/w/main.c", 3, 12, Level::Note),
        ("/w/src/io.c", 10, 1, Level::Warning),
        ("/w/boot.asm", 7, 0, Level::Error),
        ("C:\\code\\a.c", 4, 5, Level::Error),
        ("/w/Makefile", 9, 0, Level::Error),
      ]
    );
    assert_eq!(
      problems[0].message,
      "'x' undeclared (first use in this function)"
    );
    assert_eq!(problems[4].message, "'z': undeclared identifier");
    assert_eq!(problems[5].message, "missing separator");
  }

  #[test]
  fn follows_the_folders_make_enters() {
    let problems = read(
      "C:\\w",
      "make[1]: Entering directory 'C:/w/lib'\n\
       util.c:1:1: error: oops\n\
       make[1]: Leaving directory 'C:/w/lib'\n\
       main.c:2:1: error: again\n",
    );
    assert_eq!(problems[0].file, "C:/w/lib\\util.c");
    assert_eq!(problems[1].file, "C:\\w\\main.c");
  }
}
