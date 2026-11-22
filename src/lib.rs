//! Makefiles for Flareite.
//!
//! - **The Targets view** (the Make entry in the activity bar) lists the
//!   makefiles in the open folder and their targets, the default goal
//!   first. Clicking a target goes to its rule; its buttons run it in a
//!   terminal (`make -f <file> <target>`), or build it in the background,
//!   with what the compilers report in Problems.
//! - **Building** runs make with its output followed line by line
//!   (`build`): errors and warnings in any file (gcc, clang, NASM, MSVC,
//!   and make's own) go to Problems through `publish-file-diagnostics`
//!   (API 2.2), whether or not the files are open. The status bar shows
//!   the build while it runs, and how it ended.
//! - **Recipes must start with a tab.** Lines where a recipe's tab should
//!   be but spaces are get a problem, with a fix that puts a tab there. The
//!   manifest's `indentation = "tabs"` has Tab insert tabs in makefiles.
//! - **Completion and hover** for variables (the file's, the files it
//!   includes', and make's own), make's functions after `$(`, automatic
//!   variables (`$@`), and targets in a rule's prerequisites.

mod build;
mod docs;
mod parse;

use std::collections::BTreeMap;

use flareite_extension_api::events::Event;
use flareite_extension_api::language_features::{
  CodeAction, CodeActionRequest, CompletionItem, CompletionKind, CompletionRequest, HoverRequest,
};
use flareite_extension_api::processes::{
  self, FinishedProcess, ProcessError, ProcessLines, ProcessOptions,
};
use flareite_extension_api::prompts::{self, AnswerValue, PickItem, PromptAnswer};
use flareite_extension_api::terminals::{self, TerminalOptions};
use flareite_extension_api::types::{
  Activation, CommandContext, ConfigValue, Diagnostic, DocumentId, DocumentInfo, LogLevel,
  MessageLevel, Position, Range, Severity, StatusItem, TextEdit,
};
use flareite_extension_api::views::{self, Expansion, ItemIcon, TreeItem};
use flareite_extension_api::workspace::FileChanges;
use flareite_extension_api::{Extension, files, host, register, workbench, workspace};

use build::{Level, Reader};
use parse::Outline;

const LANGUAGE: &str = "makefile";
const VIEW: &str = "targets";
const STATUS: &str = "build";
/// The names make looks for, in its order, and others it reads.
const NAMES: &[&str] = &["GNUmakefile", "makefile", "Makefile", "BSDmakefile"];
/// Most makefiles the view lists.
const MAX_MAKEFILES: usize = 50;
const SPACES_CODE: &str = "recipe-spaces";

fn log(message: &str) {
  host::log(LogLevel::Info, message);
}

fn split_path(path: &str) -> (&str, &str) {
  match path.rfind(['/', '\\']) {
    Some(i) => (&path[..i], &path[i + 1..]),
    None => ("", path),
  }
}

fn is_makefile(path: &str) -> bool {
  let (_, name) = split_path(path);
  NAMES.contains(&name) || name.ends_with(".mk") || name.ends_with(".mak")
}

/// What a command's argument names: a makefile and one of its targets
/// (`none` for its default goal), as `<file>\n<target>`.
fn argument(file: &str, target: Option<&str>) -> String {
  format!("{file}\n{}", target.unwrap_or(""))
}

fn parse_argument(argument: &str) -> (String, Option<String>) {
  match argument.split_once('\n') {
    Some((file, "")) => (file.to_owned(), None),
    Some((file, target)) => (file.to_owned(), Some(target.to_owned())),
    None => (argument.to_owned(), None),
  }
}

fn extra_arguments() -> Vec<String> {
  match host::configuration("arguments") {
    Some(ConfigValue::Text(text)) => text.split_whitespace().map(str::to_owned).collect(),
    _ => Vec::new(),
  }
}

/// make's arguments for a target of a makefile.
fn make_args(file: &str, target: Option<&str>) -> Vec<String> {
  let (_, name) = split_path(file);
  let mut args = vec!["-f".to_owned(), name.to_owned()];
  args.extend(extra_arguments());
  args.extend(target.map(str::to_owned));
  args
}

fn document_text(id: DocumentId) -> Option<(u64, String)> {
  let info = host::document(id).ok()?;
  let slice = host::read_lines(id, 0, info.line_count).ok()?;
  Some((slice.version, slice.lines.join("\n")))
}

fn line_of(id: DocumentId, line: u32) -> Option<String> {
  host::read_lines(id, line, line + 1)
    .ok()?
    .lines
    .into_iter()
    .next()
}

/// A build running in the background.
struct Build {
  process: u64,
  /// What the status bar and messages call it: `make all`.
  title: String,
  reader: Reader,
  /// The user stopped it (a stopped program may still say it succeeded).
  stopped: bool,
}

/// What a pick of targets is for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Purpose {
  Run,
  Build,
}

struct Open {
  path: String,
}

#[derive(Default)]
struct Make {
  documents: BTreeMap<DocumentId, Open>,
  watch: Option<u64>,
  build: Option<Build>,
  /// A target pick showing, and what it's for.
  pick: Option<(u64, Purpose)>,
  /// A rule to show once its makefile opens: path and line.
  reveal: Option<(String, u32)>,
}

impl Make {
  fn folder() -> Option<String> {
    workspace::folder().map(|folder| folder.path)
  }

  fn watch_folder(&mut self) {
    if let Some(watch) = self.watch.take() {
      workspace::unwatch(watch);
    }
    if let Some(folder) = Self::folder() {
      match workspace::watch(&folder) {
        Ok(watch) => self.watch = Some(watch),
        Err(error) => log(&format!("can't watch the folder: {error:?}")),
      }
    }
  }

  /// The makefiles in the open folder: those make reads by default (in any
  /// folder), shallowest first.
  fn makefiles() -> Vec<String> {
    let Some(folder) = Self::folder() else {
      return Vec::new();
    };
    let Ok(list) = files::list_files(&folder) else {
      return Vec::new();
    };
    let mut found: Vec<String> = list
      .paths
      .into_iter()
      .filter(|path| NAMES.contains(&split_path(path).1))
      .collect();
    found.sort_by_key(|path| (path.matches(['/', '\\']).count(), path.to_lowercase()));
    // One per folder, as make picks: GNUmakefile, makefile, then Makefile.
    let mut folders = Vec::new();
    found.retain(|path| {
      let folder = split_path(path).0.to_owned();
      if folders.contains(&folder) {
        false
      } else {
        folders.push(folder);
        true
      }
    });
    found.truncate(MAX_MAKEFILES);
    found
  }

  fn outline_of(path: &str) -> Option<Outline> {
    files::read_file(path)
      .ok()
      .map(|text| parse::outline(&text))
  }

  /// Lists the makefiles and their targets.
  fn refresh_view(&self) {
    let folder = Self::folder();
    let makefiles = Self::makefiles();
    let mut items = Vec::new();
    for (index, path) in makefiles.iter().enumerate() {
      let Some(outline) = Self::outline_of(path) else {
        continue;
      };
      let relative = folder
        .as_deref()
        .and_then(|folder| path.strip_prefix(folder))
        .map(|rest| rest.trim_start_matches(['/', '\\']).to_owned())
        .unwrap_or_else(|| path.clone());
      let parent = format!("file:{path}");
      items.push(TreeItem {
        id: parent.clone(),
        parent: None,
        label: relative,
        description: None,
        tooltip: Some(path.clone()),
        icon: ItemIcon::File(path.clone()),
        mark: None,
        context: Some("makefile".into()),
        // The first is open; more fold away.
        expansion: if index == 0 {
          Expansion::Expanded
        } else {
          Expansion::Collapsed
        },
        command: Some("flareite.makefile.open".into()),
        argument: Some(argument(path, None)),
      });
      let default = outline.default_goal();
      for target in outline.runnable_targets() {
        let is_default = default.as_deref() == Some(&target.name);
        let mut tooltip = target.rule.clone();
        for line in target.recipe.iter().take(8) {
          tooltip.push_str("\n    ");
          tooltip.push_str(line);
        }
        if !target.doc.is_empty() {
          tooltip = format!("{}\n\n{tooltip}", target.doc);
        }
        items.push(TreeItem {
          id: format!("target:{path}\n{}", target.name),
          parent: Some(parent.clone()),
          label: target.name.clone(),
          description: if is_default {
            Some("default".into())
          } else {
            target.doc.lines().next().map(str::to_owned)
          },
          tooltip: Some(tooltip),
          icon: ItemIcon::Named("play".into()),
          mark: None,
          context: Some("target".into()),
          expansion: Expansion::None,
          command: Some("flareite.makefile.open".into()),
          argument: Some(argument(path, Some(&target.name))),
        });
      }
    }
    if let Err(error) = views::set_items(VIEW, &items) {
      log(&format!("couldn't list the targets: {error:?}"));
    }
    let message: Option<String> = if folder.is_none() {
      Some("Open a folder with a Makefile to see its targets.".into())
    } else if makefiles.is_empty() {
      Some("No Makefile in the open folder.".into())
    } else {
      None
    };
    let _ = workbench::set_view_message(VIEW, message.as_deref());
  }

  /// Opens a makefile, at a target's rule if one is named.
  fn open(&mut self, file: &str, target: Option<&str>) -> Result<(), String> {
    let line = target
      .and_then(|target| {
        Self::outline_of(file)?
          .targets
          .iter()
          .find(|t| t.name == target)
          .map(|t| t.line)
      })
      .unwrap_or(0);
    // Open already: show it there; otherwise once it opens.
    let open = host::documents()
      .unwrap_or_default()
      .into_iter()
      .find(|info| info.path.as_deref() == Some(file));
    workspace::open_file(file).map_err(|error| format!("{error:?}"))?;
    match open {
      Some(info) => reveal(info.id, line),
      None => self.reveal = Some((file.to_owned(), line)),
    }
    Ok(())
  }

  fn run_in_terminal(file: &str, target: Option<&str>) -> Result<(), String> {
    let (folder, _) = split_path(file);
    let title = format!("make {}", target.unwrap_or(""));
    terminals::run(
      "make",
      &make_args(file, target),
      &TerminalOptions {
        title: Some(title.trim().to_owned()),
        cwd: Some(folder.to_owned()),
        env: Vec::new(),
      },
    )
    .map_err(|error| describe(&error))
  }

  fn start_build(&mut self, file: &str, target: Option<&str>) -> Result<(), String> {
    if let Some(build) = &self.build {
      return Err(format!(
        "{} is still running; stop it first (Make: Stop Build).",
        build.title
      ));
    }
    let (folder, _) = split_path(file);
    let title = format!("make {}", target.unwrap_or("")).trim().to_owned();
    let options = ProcessOptions {
      cwd: Some(folder.to_owned()),
      env: Vec::new(),
      stdin: None,
      timeout_ms: None,
    };
    let process = processes::spawn_streaming("make", &make_args(file, target), &options)
      .map_err(|error| describe(&error))?;
    // This build's problems replace the last one's.
    host::clear_file_diagnostics();
    LAST_ARGUMENT.with_borrow_mut(|last| *last = Some(argument(file, target)));
    self.build = Some(Build {
      process,
      title: title.clone(),
      reader: Reader::new(folder),
      stopped: false,
    });
    set_status(
      &format!("{title}: building…"),
      "Building; click to stop",
      "flareite.makefile.stopBuild",
    );
    log(&format!("{title} started in {folder}"));
    Ok(())
  }

  fn output(&mut self, lines: ProcessLines) {
    let Some(build) = self
      .build
      .as_mut()
      .filter(|build| build.process == lines.process)
    else {
      return;
    };
    for line in &lines.lines {
      host::log(LogLevel::Debug, &line.text);
      build.reader.line(&line.text);
    }
  }

  fn finished(&mut self, finished: FinishedProcess) {
    let Some(build) = self
      .build
      .take_if(|build| build.process == finished.process)
    else {
      return;
    };
    let mut by_file: BTreeMap<String, Vec<Diagnostic>> = BTreeMap::new();
    let (mut errors, mut warnings) = (0, 0);
    for problem in &build.reader.problems {
      let severity = match problem.level {
        Level::Error => {
          errors += 1;
          Severity::Error
        }
        Level::Warning => {
          warnings += 1;
          Severity::Warning
        }
        Level::Note => Severity::Information,
      };
      let line = problem.line.saturating_sub(1);
      let column = problem.column.saturating_sub(1);
      // Without a column, the whole line; with one, from it to the line's
      // end (the editor keeps it inside the line).
      by_file
        .entry(problem.file.clone())
        .or_default()
        .push(Diagnostic {
          range: Range {
            start: Position { line, column },
            end: Position {
              line,
              column: u32::MAX / 2,
            },
          },
          severity,
          message: problem.message.clone(),
          code: None,
        });
    }
    for (file, diagnostics) in &by_file {
      if let Err(error) = host::publish_file_diagnostics(file, diagnostics) {
        log(&format!("couldn't report problems in {file}: {error:?}"));
      }
    }
    let ended = match &finished.outcome {
      _ if build.stopped => "was stopped".to_owned(),
      Ok(output) if output.exit_code == Some(0) => "succeeded".to_owned(),
      Ok(output) => match output.exit_code {
        Some(code) => format!("failed (exit code {code})"),
        None => "was stopped".to_owned(),
      },
      Err(ProcessError::Killed) => "was stopped".to_owned(),
      Err(error) => format!("couldn't run: {}", describe(error)),
    };
    let mut summary = format!("{} {ended}", build.title);
    if errors + warnings > 0 {
      summary.push_str(&format!(
        ": {errors} {}, {warnings} {}",
        if errors == 1 { "error" } else { "errors" },
        if warnings == 1 { "warning" } else { "warnings" }
      ));
    }
    log(&summary);
    let outcome = if errors + warnings > 0 {
      format!(
        "{errors} {}, {warnings} {}",
        if errors == 1 { "error" } else { "errors" },
        if warnings == 1 { "warning" } else { "warnings" }
      )
    } else if ended == "succeeded" {
      "done".to_owned()
    } else if build.stopped {
      "stopped".to_owned()
    } else {
      "failed".to_owned()
    };
    set_status(
      &format!("{}: {outcome}", build.title),
      &format!("{summary}. Click to build again."),
      "flareite.makefile.buildAgain",
    );
    let level = if ended == "succeeded" {
      MessageLevel::Info
    } else {
      MessageLevel::Warning
    };
    if !build.stopped && (ended != "succeeded" || errors + warnings > 0) {
      host::show_message(
        level,
        &format!("{summary}. The problems are in View: Problems."),
      );
    }
  }

  /// Checks a makefile document for recipes indented with spaces.
  fn lint(&self, id: DocumentId) {
    let Some((version, text)) = document_text(id) else {
      return;
    };
    let outline = parse::outline(&text);
    let lines: Vec<&str> = text.lines().collect();
    let diagnostics: Vec<Diagnostic> = outline
      .spaced_recipes
      .iter()
      .map(|spaced| Diagnostic {
        range: Range {
          start: Position {
            line: spaced.line,
            column: 0,
          },
          end: Position {
            line: spaced.line,
            column: lines
              .get(spaced.line as usize)
              .map_or(spaced.indent, |line| line.len() as u32),
          },
        },
        severity: Severity::Error,
        message:
          "Recipe lines must start with a tab, not spaces: make stops with \"missing separator\"."
            .into(),
        code: Some(SPACES_CODE.into()),
      })
      .collect();
    let _ = host::publish_diagnostics(id, version, &diagnostics);
  }

  fn opened(&mut self, info: &DocumentInfo) {
    if info.language.as_deref() != Some(LANGUAGE) {
      return;
    }
    let path = info.path.clone().unwrap_or_default();
    if let Some((file, line)) = self.reveal.take_if(|(file, _)| *file == path) {
      let _ = file;
      reveal(info.id, line);
    }
    self.documents.insert(info.id, Open { path });
    self.lint(info.id);
  }

  /// The variables and targets a makefile document and the files it
  /// includes define.
  fn outlines(&self, id: DocumentId) -> Vec<(Outline, Option<String>)> {
    let Some((_, text)) = document_text(id) else {
      return Vec::new();
    };
    let outline = parse::outline(&text);
    let folder = self
      .documents
      .get(&id)
      .map(|open| split_path(&open.path).0.to_owned())
      .unwrap_or_default();
    let mut found = Vec::new();
    for (include, _) in &outline.includes {
      if include.contains('$') || folder.is_empty() {
        continue;
      }
      let path = build::join(&folder, include);
      if let Ok(text) = files::read_file(&path) {
        found.push((parse::outline(&text), Some(include.clone())));
      }
    }
    found.insert(0, (outline, None));
    found
  }

  fn pick_target(&mut self, purpose: Purpose) -> Result<(), String> {
    let mut items = Vec::new();
    for file in Self::makefiles() {
      let Some(outline) = Self::outline_of(&file) else {
        continue;
      };
      let default = outline.default_goal();
      let (_, name) = split_path(&file);
      let relative = Self::folder()
        .and_then(|folder| file.strip_prefix(&folder).map(str::to_owned))
        .map(|rest| rest.trim_start_matches(['/', '\\']).to_owned())
        .unwrap_or_else(|| name.to_owned());
      for target in outline.runnable_targets() {
        let mut description = relative.clone();
        if default.as_deref() == Some(&target.name) {
          description.push_str(" · default");
        }
        items.push(PickItem {
          label: target.name.clone(),
          description: Some(description),
          value: argument(&file, Some(&target.name)),
        });
      }
    }
    if items.is_empty() {
      return Err("No Makefile in the open folder has targets.".into());
    }
    items.truncate(1000);
    let title = match purpose {
      Purpose::Run => "Run a target in a terminal",
      Purpose::Build => "Build a target, with its problems in Problems",
    };
    let prompt = prompts::pick(title, &items).map_err(|error| format!("{error:?}"))?;
    self.pick = Some((prompt, purpose));
    Ok(())
  }

  fn answered(&mut self, answer: PromptAnswer) -> Result<(), String> {
    let Some((_, purpose)) = self.pick.take_if(|(prompt, _)| *prompt == answer.prompt) else {
      return Ok(());
    };
    let AnswerValue::Picked(value) = answer.value else {
      return Ok(());
    };
    let (file, target) = parse_argument(&value);
    match purpose {
      Purpose::Run => Self::run_in_terminal(&file, target.as_deref()),
      Purpose::Build => self.start_build(&file, target.as_deref()),
    }
  }
}

thread_local! {
  /// The last build's makefile and target (as a command's argument), to
  /// build again.
  static LAST_ARGUMENT: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

fn reveal(id: DocumentId, line: u32) {
  let at = Position { line, column: 0 };
  let _ = workbench::reveal(id, Range { start: at, end: at });
}

/// The status bar item as the manifest has it.
fn reset_status() {
  set_status(
    "Make",
    "Build a target with make",
    "flareite.makefile.buildAgain",
  );
}

fn set_status(text: &str, tooltip: &str, command: &str) {
  let _ = workbench::set_status_item(
    STATUS,
    Some(&StatusItem {
      text: text.into(),
      tooltip: Some(tooltip.into()),
      command: Some(command.into()),
    }),
  );
}

fn describe(error: &ProcessError) -> String {
  match error {
    ProcessError::NotFound(why) => format!(
      "make couldn't be run ({why}). Install GNU make and put it on the PATH, or set where it is in settings.toml: [extensions.programs] \"flareite.makefile.make\" = \"C:/path/to/make.exe\" (mingw32-make works too)."
    ),
    ProcessError::Untrusted => "make runs only in a folder you trust.".into(),
    ProcessError::PermissionDenied => {
      "The Makefile extension isn't allowed to run programs.".into()
    }
    other => format!("make failed: {other:?}"),
  }
}

/// The name of a variable or function being typed after `$(` or `${`:
/// where it starts, if the cursor is in one.
fn reference_start(before: &str) -> Option<(usize, char)> {
  let bytes = before.as_bytes();
  let mut i = bytes.len();
  while i > 0 {
    let c = bytes[i - 1];
    if c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'-') {
      i -= 1;
      continue;
    }
    if (c == b'(' || c == b'{') && i >= 2 && bytes[i - 2] == b'$' {
      return Some((i, c as char));
    }
    return None;
  }
  None
}

fn is_word_char(c: char) -> bool {
  c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | '/')
}

/// The word at a byte column, with where it starts.
fn word_at(line: &str, column: usize) -> Option<(usize, &str)> {
  let column = column.min(line.len());
  let start = line[..column]
    .char_indices()
    .rev()
    .take_while(|&(_, c)| is_word_char(c))
    .last()
    .map_or(column, |(i, _)| i);
  let end = line[column..]
    .char_indices()
    .find(|&(_, c)| !is_word_char(c))
    .map_or(line.len(), |(i, _)| column + i);
  (end > start).then(|| (start, &line[start..end]))
}

fn code_block(text: &str) -> String {
  format!("```makefile\n{text}\n```")
}

impl Extension for Make {
  fn activate(_: Activation) -> Result<Self, String> {
    let mut make = Make::default();
    make.watch_folder();
    make.refresh_view();
    Ok(make)
  }

  fn run_command(&mut self, command: &str, context: CommandContext) -> Result<(), String> {
    let target = context.argument.as_deref().map(parse_argument);
    match command {
      "flareite.makefile.open" => {
        let (file, target) = target.ok_or("Choose a target in the Targets view.")?;
        self.open(&file, target.as_deref())
      }
      "flareite.makefile.runTarget" => match target {
        Some((file, target)) => Self::run_in_terminal(&file, target.as_deref()),
        None => self.pick_target(Purpose::Run),
      },
      "flareite.makefile.buildTarget" => match target {
        Some((file, target)) => self.start_build(&file, target.as_deref()),
        None => self.pick_target(Purpose::Build),
      },
      "flareite.makefile.build" => {
        let file = Self::makefiles()
          .into_iter()
          .next()
          .ok_or("No Makefile in the open folder.")?;
        self.start_build(&file, None)
      }
      "flareite.makefile.buildAgain" => {
        let last = LAST_ARGUMENT.with_borrow(Clone::clone);
        match last {
          Some(last) => {
            let (file, target) = parse_argument(&last);
            self.start_build(&file, target.as_deref())
          }
          None => self.pick_target(Purpose::Build),
        }
      }
      "flareite.makefile.stopBuild" => {
        match &mut self.build {
          Some(build) => {
            build.stopped = true;
            let _ = processes::kill(build.process);
          }
          None => {
            reset_status();
          }
        }
        Ok(())
      }
      "flareite.makefile.clearProblems" => {
        host::clear_file_diagnostics();
        reset_status();
        Ok(())
      }
      "flareite.makefile.refresh" => {
        self.refresh_view();
        Ok(())
      }
      other => Err(format!("unknown command {other}")),
    }
  }

  fn handle_event(&mut self, event: Event) -> Result<(), String> {
    match event {
      Event::DocumentOpened(info) => self.opened(&info),
      Event::DocumentChanged(change) => {
        if self.documents.contains_key(&change.document) {
          self.lint(change.document);
        }
      }
      Event::DocumentClosed(id) => {
        self.documents.remove(&id);
      }
      Event::FilesChanged(FileChanges {
        watch,
        paths,
        overflow,
      }) if Some(watch) == self.watch => {
        if overflow || paths.iter().any(|path| is_makefile(path)) {
          self.refresh_view();
        }
      }
      Event::FolderChanged => {
        self.watch_folder();
        self.refresh_view();
      }
      Event::ProcessOutput(lines) => self.output(lines),
      Event::ProcessFinished(finished) => self.finished(finished),
      Event::PromptAnswered(answer) => self.answered(answer)?,
      _ => {}
    }
    Ok(())
  }

  fn provide_completions(
    &mut self,
    request: CompletionRequest,
  ) -> Result<Vec<CompletionItem>, String> {
    let Some(line) = line_of(request.document, request.position.line) else {
      return Ok(Vec::new());
    };
    let column = (request.position.column as usize).min(line.len());
    if !line.is_char_boundary(column) {
      return Ok(Vec::new());
    }
    let before = &line[..column];
    if parse::comment_start(before).is_some() {
      return Ok(Vec::new());
    }
    let outlines = self.outlines(request.document);
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let start_at = |start: usize| Position {
      line: request.position.line,
      column: start as u32,
    };
    if let Some((start, bracket)) = reference_start(before) {
      for (outline, file) in &outlines {
        for variable in &outline.variables {
          if seen.insert(variable.name.clone()) {
            items.push(CompletionItem {
              label: variable.name.clone(),
              kind: CompletionKind::Variable,
              detail: Some(match file {
                Some(file) => format!("{} · {file}", variable.source),
                None => variable.source.clone(),
              }),
              start: start_at(start),
              insert: variable.name.clone(),
              filter: None,
              sort: Some(format!("0{}", variable.name)),
              deprecated: false,
            });
          }
        }
      }
      for (name, text) in docs::VARIABLES {
        if seen.insert((*name).to_owned()) {
          items.push(CompletionItem {
            label: (*name).to_owned(),
            kind: CompletionKind::Variable,
            detail: Some((*text).to_owned()),
            start: start_at(start),
            insert: (*name).to_owned(),
            filter: None,
            sort: Some(format!("1{name}")),
            deprecated: false,
          });
        }
      }
      // Functions take arguments after a space: `$(patsubst ...`.
      if bracket == '(' {
        for (name, usage, _) in docs::FUNCTIONS {
          items.push(CompletionItem {
            label: (*name).to_owned(),
            kind: CompletionKind::Function,
            detail: Some((*usage).to_owned()),
            start: start_at(start),
            insert: format!("{name} "),
            filter: Some((*name).to_owned()),
            sort: Some(format!("2{name}")),
            deprecated: false,
          });
        }
      }
      return Ok(items);
    }
    // Prerequisites: after a rule's colon, outside a recipe.
    let in_rule = !line.starts_with('\t')
      && before.contains(':')
      && parse::assignment(before).is_none()
      && !before.contains(":=");
    if in_rule {
      let start = before
        .rfind(|c: char| c.is_whitespace() || c == ':')
        .map_or(0, |i| i + 1);
      for (outline, _) in &outlines {
        for target in outline.runnable_targets() {
          if seen.insert(target.name.clone()) {
            items.push(CompletionItem {
              label: target.name.clone(),
              kind: CompletionKind::File,
              detail: Some(target.rule.clone()),
              start: start_at(start),
              insert: target.name.clone(),
              filter: None,
              sort: None,
              deprecated: false,
            });
          }
        }
      }
    }
    Ok(items)
  }

  fn provide_hover(&mut self, request: HoverRequest) -> Result<Option<String>, String> {
    let Some(line) = line_of(request.document, request.position.line) else {
      return Ok(None);
    };
    let column = (request.position.column as usize).min(line.len());
    if !line.is_char_boundary(column) {
      return Ok(None);
    }
    if parse::comment_start(&line).is_some_and(|start| start <= column) {
      return Ok(None);
    }
    // `$@`, `$<`, and the other automatic variables: the cursor on the `$`
    // or the character after it.
    let bytes = line.as_bytes();
    for dollar in [Some(column), column.checked_sub(1)].into_iter().flatten() {
      if bytes.get(dollar) == Some(&b'$')
        && let Some(&c) = bytes.get(dollar + 1)
        && let Some(text) = docs::automatic(&(c as char).to_string())
      {
        return Ok(Some(format!(
          "**`${}`** (automatic variable)\n\n{text}",
          c as char
        )));
      }
    }
    let Some((start, word)) = word_at(&line, column) else {
      return Ok(None);
    };
    let after_reference = line[..start].ends_with("$(") || line[..start].ends_with("${");
    // A function: `$(name ` with a space after.
    if line[..start].ends_with("$(")
      && line[start + word.len()..].starts_with(' ')
      && let Some((_, usage, text)) = docs::function(word)
    {
      return Ok(Some(format!("{}\n\n{text}", code_block(usage))));
    }
    let outlines = self.outlines(request.document);
    let definitions: Vec<String> = outlines
      .iter()
      .flat_map(|(outline, file)| {
        outline
          .variables
          .iter()
          .filter(|variable| variable.name == word)
          .map(move |variable| {
            let place = match file {
              Some(file) => format!("{file}, line {}", variable.line + 1),
              None => format!("line {}", variable.line + 1),
            };
            let mut text = format!("{}\n\n*{place}*", code_block(&variable.source));
            if !variable.doc.is_empty() {
              text.push_str("\n\n");
              text.push_str(&variable.doc);
            }
            text
          })
      })
      .collect();
    if after_reference || !definitions.is_empty() {
      let mut parts = definitions;
      if let Some(text) = docs::variable(word) {
        parts.push(text.to_owned());
      }
      if !parts.is_empty() {
        return Ok(Some(parts.join("\n\n---\n\n")));
      }
    }
    // A target.
    for (outline, file) in &outlines {
      if let Some(target) = outline.targets.iter().find(|target| target.name == word) {
        let mut source = target.rule.clone();
        for line in target.recipe.iter().take(12) {
          source.push_str("\n\t");
          source.push_str(line);
        }
        let place = match file {
          Some(file) => format!("{file}, line {}", target.line + 1),
          None => format!("line {}", target.line + 1),
        };
        let mut text = format!("{}\n\n*target* · {place}", code_block(&source));
        if outline.phony.contains(&target.name) {
          text.push_str(" · phony");
        }
        if !target.doc.is_empty() {
          text.push_str("\n\n");
          text.push_str(&target.doc);
        }
        return Ok(Some(text));
      }
    }
    Ok(None)
  }

  fn provide_code_actions(
    &mut self,
    request: CodeActionRequest,
  ) -> Result<Vec<CodeAction>, String> {
    let Some((version, text)) = document_text(request.document) else {
      return Ok(Vec::new());
    };
    if version != request.version {
      return Ok(Vec::new());
    }
    let outline = parse::outline(&text);
    let (first, last) = (request.range.start.line, request.range.end.line);
    let fix = |spaced: &parse::SpacedRecipe| TextEdit {
      range: Range {
        start: Position {
          line: spaced.line,
          column: 0,
        },
        end: Position {
          line: spaced.line,
          column: spaced.indent,
        },
      },
      text: "\t".into(),
    };
    let mut actions = Vec::new();
    if let Some(here) = outline
      .spaced_recipes
      .iter()
      .find(|spaced| (first..=last).contains(&spaced.line))
    {
      actions.push(CodeAction {
        title: "Indent the recipe line with a tab".into(),
        kind: Some("quickfix".into()),
        preferred: true,
        edits: vec![fix(here)],
        command: None,
        argument: None,
      });
    }
    if outline.spaced_recipes.len() > 1 {
      actions.push(CodeAction {
        title: "Indent every recipe line with a tab".into(),
        kind: Some("quickfix".into()),
        preferred: false,
        edits: outline.spaced_recipes.iter().map(fix).collect(),
        command: None,
        argument: None,
      });
    }
    Ok(actions)
  }
}

register!(Make);

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn finds_references_being_typed() {
    assert_eq!(reference_start("\t$(CC) $(CF"), Some((9, '(')));
    assert_eq!(reference_start("x := ${"), Some((7, '{')));
    assert_eq!(reference_start("x := $(a b"), None);
    assert_eq!(reference_start("all: app"), None);
  }

  #[test]
  fn finds_words_and_arguments() {
    assert_eq!(word_at("all: app lib/x.o", 11), Some((9, "lib/x.o")));
    assert_eq!(word_at("$(CFLAGS)", 4), Some((2, "CFLAGS")));
    assert_eq!(
      parse_argument(&argument("C:\\a\\Makefile", Some("all"))),
      ("C:\\a\\Makefile".into(), Some("all".into()))
    );
    assert_eq!(parse_argument(&argument("/Makefile", None)).1, None);
    assert!(is_makefile("/w/sub/Makefile") && is_makefile("rules.mk") && !is_makefile("make.c"));
  }
}
