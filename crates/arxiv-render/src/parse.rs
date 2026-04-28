use math_render::{MathInput, render};
use std::collections::HashMap;

const WRAP_WIDTH: usize = 80;

/// Convert a set of `.tex` source files into display lines with math
/// pre-rendered as Unicode.
pub fn to_lines(sources: Vec<(String, String)>) -> Vec<String> {
  let file_map: HashMap<String, String> = sources.into_iter().collect();
  let root = find_root(&file_map);
  let expanded = expand_inputs(&root, &file_map, 0);
  let body = extract_body(&expanded);
  process(&body)
}

// ── Root selection ────────────────────────────────────────────────────────────

fn find_root(files: &HashMap<String, String>) -> String {
  // Prefer the file with \begin{document}.
  for content in files.values() {
    if content.contains(r"\begin{document}") {
      return content.clone();
    }
  }
  // Fallback: largest file.
  files
    .values()
    .max_by_key(|c| c.len())
    .cloned()
    .unwrap_or_default()
}

// ── \input{} resolution ───────────────────────────────────────────────────────

fn expand_inputs(content: &str, files: &HashMap<String, String>, depth: usize) -> String {
  if depth > 10 {
    return content.to_string(); // guard against circular includes
  }
  let mut out = String::with_capacity(content.len());
  let mut rest = content;
  while let Some(pos) = rest.find(r"\input{") {
    out.push_str(&rest[..pos]);
    rest = &rest[pos + 7..];
    if let Some(end) = rest.find('}') {
      let filename = rest[..end].trim();
      rest = &rest[end + 1..];
      let resolved = resolve_input(filename, files);
      if let Some(included) = resolved {
        out.push_str(&expand_inputs(&included, files, depth + 1));
      }
    }
  }
  out.push_str(rest);
  out
}

fn resolve_input(name: &str, files: &HashMap<String, String>) -> Option<String> {
  // Try exact name, then with .tex appended, then basename only.
  let candidates = [
    name.to_string(),
    format!("{name}.tex"),
    std::path::Path::new(name)
      .file_name()
      .map(|n| n.to_string_lossy().to_string())
      .unwrap_or_default(),
    format!(
      "{}.tex",
      std::path::Path::new(name)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
    ),
  ];
  for c in &candidates {
    if let Some(content) = files.get(c.as_str()) {
      return Some(content.clone());
    }
  }
  None
}

// ── Document body extraction ──────────────────────────────────────────────────

fn extract_body(content: &str) -> String {
  let start = content
    .find(r"\begin{document}")
    .map(|p| p + r"\begin{document}".len())
    .unwrap_or(0);
  let end = content
    .rfind(r"\end{document}")
    .unwrap_or(content.len());
  content[start..end].to_string()
}

// ── Main processor ────────────────────────────────────────────────────────────

fn process(body: &str) -> Vec<String> {
  let mut out: Vec<String> = Vec::new();

  // Abstract block collected first so it appears at the top.
  if let Some(abs) = extract_env(body, "abstract") {
    out.push(String::new());
    out.push("── Abstract ──".to_string());
    for line in process_prose(&abs) {
      out.push(line);
    }
    out.push(String::new());
  }

  // Process the body character by character using a mini state machine.
  let processed = process_body(body);
  out.extend(processed);
  out
}

// ── Body state machine ────────────────────────────────────────────────────────

fn process_body(body: &str) -> Vec<String> {
  let mut out: Vec<String> = Vec::new();
  let mut current_line = String::new();

  // Track which block environments to skip entirely.
  let skip_envs = ["figure", "table", "lstlisting", "verbatim", "tikzpicture", "algorithm"];
  // Track which math environments to render as display blocks.
  let display_math_envs = ["equation", "equation*", "align", "align*", "aligned",
                            "gather", "gather*", "multline", "multline*", "eqnarray",
                            "eqnarray*"];

  let mut i = 0usize;
  let text: Vec<char> = body.chars().collect();
  let len = text.len();

  while i < len {
    let c = text[i];

    // LaTeX comment: skip to end of line.
    if c == '%' && (i == 0 || text[i - 1] != '\\') {
      while i < len && text[i] != '\n' {
        i += 1;
      }
      continue;
    }

    // Backslash command.
    if c == '\\' && i + 1 < len {
      let (cmd, consumed) = read_command(&text, i + 1);
      i += 1 + consumed;

      match cmd.as_str() {
        // Skip \end{...} — block ends handled inline.
        "end" => {
          let (env, skip) = read_braced_arg(&text, i);
          i += skip;
          // If ending a skip env, push placeholder.
          if skip_envs.contains(&env.trim()) {
            flush_line(&mut current_line, &mut out);
            out.push(format!("[{env}]"));
          }
          continue;
        }
        "begin" => {
          let (env, skip) = read_braced_arg(&text, i);
          i += skip;
          let env = env.trim().to_string();

          // Display math environments.
          if display_math_envs.iter().any(|e| *e == env.as_str()) {
            flush_line(&mut current_line, &mut out);
            let (math, adv) = read_until_end(&text, i, &env);
            i += adv;
            let rendered = render(MathInput::Latex(math.trim()));
            for l in rendered.lines() {
              out.push(l.to_string());
            }
            continue;
          }

          // Abstract handled at top — skip here.
          if env == "abstract" {
            let (_abs, adv) = read_until_end(&text, i, "abstract");
            i += adv;
            continue;
          }

          // Skipped environments.
          if skip_envs.contains(&env.as_str()) {
            let (_content, adv) = read_until_end(&text, i, &env);
            i += adv;
            continue;
          }

          // Everything else (itemize, enumerate, etc.) — just continue.
          continue;
        }

        // Section headers.
        "section" => {
          let (title, skip) = read_braced_arg(&text, i);
          i += skip;
          flush_line(&mut current_line, &mut out);
          out.push(String::new());
          out.push(format!("══ {} ══", title.trim()));
          out.push(String::new());
        }
        "subsection" => {
          let (title, skip) = read_braced_arg(&text, i);
          i += skip;
          flush_line(&mut current_line, &mut out);
          out.push(String::new());
          out.push(format!("── {} ──", title.trim()));
          out.push(String::new());
        }
        "subsubsection" | "paragraph" => {
          let (title, skip) = read_braced_arg(&text, i);
          i += skip;
          flush_line(&mut current_line, &mut out);
          out.push(format!("─ {} ─", title.trim()));
        }

        // Inline text commands — keep the argument, drop the command.
        "emph" | "textbf" | "textit" | "texttt" | "text" | "mathrm"
        | "mathbf" | "mathit" | "mathcal" | "mathbb" => {
          let (content, skip) = read_braced_arg(&text, i);
          i += skip;
          current_line.push_str(&content);
        }

        // Two-arg formatting commands: skip first arg (option/color), keep second.
        "textcolor" | "colorbox" | "fbox" | "mbox" | "makebox" => {
          let (_opt, skip1) = read_braced_arg(&text, i);
          i += skip1;
          let (content, skip2) = read_braced_arg(&text, i);
          i += skip2;
          current_line.push_str(&content);
        }

        // Commands with args to completely discard (preamble-style in body).
        "color" | "bibliography" | "bibliographystyle" | "maketitle"
        | "tableofcontents" | "newcommand" | "renewcommand" | "setlength"
        | "addtolength" | "setcounter" | "addtocounter" | "usepackage"
        | "RequirePackage" | "PassOptionsToPackage" | "geometry"
        | "vspace*" | "hspace" | "hspace*" | "rule" | "includegraphics"
        | "captionsetup" | "caption" | "subcaption" => {
          // Consume all consecutive braced args but discard content.
          while i < len && (text[i] == '{' || text[i] == '[') {
            if text[i] == '[' {
              // Optional arg [...].
              while i < len && text[i] != ']' { i += 1; }
              if i < len { i += 1; }
            } else {
              let (_, skip) = read_braced_arg(&text, i);
              i += skip;
            }
          }
        }

        // Citations / refs → short placeholder.
        "cite" | "citep" | "citet" | "citealt" => {
          let (_key, skip) = read_braced_arg(&text, i);
          i += skip;
          current_line.push_str("[ref]");
        }
        "ref" | "eqref" => {
          let (_key, skip) = read_braced_arg(&text, i);
          i += skip;
          current_line.push_str("[§]");
        }
        "label" => {
          let (_key, skip) = read_braced_arg(&text, i);
          i += skip;
          // silently drop
        }
        "footnote" => {
          let (note, skip) = read_braced_arg(&text, i);
          i += skip;
          current_line.push_str(" [note: ");
          current_line.push_str(note.trim());
          current_line.push(']');
        }
        "url" | "href" => {
          let (url, skip) = read_braced_arg(&text, i);
          i += skip;
          // For \href, skip the display text braced arg.
          if cmd == "href" {
            let (_display, skip2) = read_braced_arg(&text, i);
            i += skip2;
          }
          current_line.push_str(url.trim());
        }

        // Inline math \( ... \)
        "(" => {
          let (math, adv) = read_until_str(&text, i, r"\)");
          i += adv;
          let rendered = render(MathInput::Latex(math.trim()));
          current_line.push_str(&rendered);
        }
        // Display math \[ ... \]
        "[" => {
          let (math, adv) = read_until_str(&text, i, r"\]");
          i += adv;
          flush_line(&mut current_line, &mut out);
          let rendered = render(MathInput::Latex(math.trim()));
          for l in rendered.lines() {
            out.push(l.to_string());
          }
        }

        // Newline commands.
        "\\" | "newline" | "hline" => {
          flush_line(&mut current_line, &mut out);
        }
        "par" | "medskip" | "bigskip" | "smallskip" | "vspace" | "vskip" => {
          let _ = if cmd == "vspace" || cmd == "vskip" {
            let (_arg, skip) = read_braced_arg(&text, i);
            i += skip;
          };
          flush_line(&mut current_line, &mut out);
          out.push(String::new());
        }

        // Everything else — silently drop the command, keep args if any.
        _ => {
          // If followed by braced arg, keep the content.
          if i < len && text[i] == '{' {
            let (content, skip) = read_braced_arg(&text, i);
            i += skip;
            current_line.push_str(&content);
          }
        }
      }
      continue;
    }

    // Inline math $...$ (single dollar sign, not $$).
    if c == '$' {
      if i + 1 < len && text[i + 1] == '$' {
        // Display math $$ ... $$.
        i += 2;
        let (math, adv) = read_until_double_dollar(&text, i);
        i += adv;
        flush_line(&mut current_line, &mut out);
        let rendered = render(MathInput::Latex(math.trim()));
        for l in rendered.lines() {
          out.push(l.to_string());
        }
      } else {
        // Inline math $ ... $.
        i += 1;
        let (math, adv) = read_until_single_dollar(&text, i);
        i += adv;
        let rendered = render(MathInput::Latex(math.trim()));
        current_line.push_str(&rendered);
      }
      continue;
    }

    // Newline in source — collapse multiple blanks.
    if c == '\n' {
      if i + 1 < len && text[i + 1] == '\n' {
        flush_line(&mut current_line, &mut out);
        out.push(String::new());
        // Skip additional blank lines.
        while i < len && text[i] == '\n' {
          i += 1;
        }
        continue;
      } else {
        current_line.push(' ');
      }
    } else {
      current_line.push(c);
    }
    i += 1;
  }

  flush_line(&mut current_line, &mut out);

  // Word-wrap all prose lines at WRAP_WIDTH.
  wrap_lines(out)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn flush_line(line: &mut String, out: &mut Vec<String>) {
  let trimmed = line.trim().to_string();
  if !trimmed.is_empty() {
    out.push(trimmed);
  }
  line.clear();
}

fn wrap_lines(lines: Vec<String>) -> Vec<String> {
  let mut out = Vec::new();
  for line in lines {
    if line.is_empty() {
      out.push(String::new());
      continue;
    }
    // Don't wrap header lines.
    if line.starts_with("══") || line.starts_with("──") || line.starts_with('─') {
      out.push(line);
      continue;
    }
    for wrapped in textwrap::wrap(&line, WRAP_WIDTH) {
      out.push(wrapped.to_string());
    }
  }
  out
}

/// Read a LaTeX command name starting at position `start` in `text`.
/// Returns (command_name, chars_consumed_including_command).
fn read_command(text: &[char], start: usize) -> (String, usize) {
  let mut cmd = String::new();
  let mut i = start;
  // Single non-alpha char commands: \\, \[, \(, \%, etc.
  if i < text.len() && !text[i].is_alphabetic() {
    return (text[i].to_string(), 1);
  }
  while i < text.len() && text[i].is_alphabetic() {
    cmd.push(text[i]);
    i += 1;
  }
  // Skip trailing whitespace after command name.
  while i < text.len() && text[i] == ' ' {
    i += 1;
  }
  (cmd, i - start)
}

/// Read `{...}` argument at position `start`. Returns (content, chars_consumed).
fn read_braced_arg(text: &[char], start: usize) -> (String, usize) {
  if start >= text.len() || text[start] != '{' {
    return (String::new(), 0);
  }
  let mut depth = 0usize;
  let mut content = String::new();
  let mut i = start;
  while i < text.len() {
    match text[i] {
      '{' => {
        depth += 1;
        if depth > 1 {
          content.push('{');
        }
      }
      '}' => {
        depth -= 1;
        if depth == 0 {
          i += 1;
          break;
        }
        content.push('}');
      }
      c => content.push(c),
    }
    i += 1;
  }
  (content, i - start)
}

/// Read content until `\end{env_name}`. Returns (content, chars_consumed).
fn read_until_end(text: &[char], start: usize, env: &str) -> (String, usize) {
  let end_marker: Vec<char> = format!(r"\end{{{env}}}").chars().collect();
  let mut content = String::new();
  let mut i = start;
  while i < text.len() {
    if text[i..].starts_with(&end_marker) {
      i += end_marker.len();
      break;
    }
    content.push(text[i]);
    i += 1;
  }
  (content, i - start)
}

/// Read content until a string marker. Returns (content, chars_consumed).
fn read_until_str(text: &[char], start: usize, marker: &str) -> (String, usize) {
  let marker_chars: Vec<char> = marker.chars().collect();
  let mut content = String::new();
  let mut i = start;
  while i < text.len() {
    if text[i..].starts_with(&marker_chars) {
      i += marker_chars.len();
      break;
    }
    content.push(text[i]);
    i += 1;
  }
  (content, i - start)
}

/// Read inline math until the next unescaped `$`.
fn read_until_single_dollar(text: &[char], start: usize) -> (String, usize) {
  let mut content = String::new();
  let mut i = start;
  while i < text.len() {
    if text[i] == '$' && (i == 0 || text[i - 1] != '\\') {
      i += 1;
      break;
    }
    content.push(text[i]);
    i += 1;
  }
  (content, i - start)
}

/// Read display math until `$$`.
fn read_until_double_dollar(text: &[char], start: usize) -> (String, usize) {
  let mut content = String::new();
  let mut i = start;
  while i + 1 < text.len() {
    if text[i] == '$' && text[i + 1] == '$' {
      i += 2;
      break;
    }
    content.push(text[i]);
    i += 1;
  }
  (content, i - start)
}

/// Extract the content of a named environment from a string (used for abstract).
fn extract_env(body: &str, env: &str) -> Option<String> {
  let begin = format!(r"\begin{{{env}}}");
  let end = format!(r"\end{{{env}}}");
  let start = body.find(&begin)? + begin.len();
  let finish = body.find(&end)?;
  if start < finish {
    Some(body[start..finish].to_string())
  } else {
    None
  }
}

/// Process prose text (used for abstract) — strips simple commands, wraps.
fn process_prose(text: &str) -> Vec<String> {
  // For the abstract we run a simplified pass — just strip obvious commands.
  let mut out = String::new();
  let chars: Vec<char> = text.chars().collect();
  let mut i = 0;
  while i < chars.len() {
    // Strip % comments.
    if chars[i] == '%' && (i == 0 || chars[i - 1] != '\\') {
      while i < chars.len() && chars[i] != '\n' { i += 1; }
      continue;
    }
    if chars[i] == '\\' && i + 1 < chars.len() {
      let (cmd, consumed) = read_command(&chars, i + 1);
      i += 1 + consumed;
      match cmd.as_str() {
        "emph" | "textbf" | "textit" | "texttt" => {
          let (content, skip) = read_braced_arg(&chars, i);
          i += skip;
          out.push_str(&content);
        }
        _ => {
          if i < chars.len() && chars[i] == '{' {
            let (content, skip) = read_braced_arg(&chars, i);
            i += skip;
            out.push_str(&content);
          }
        }
      }
      continue;
    }
    if chars[i] == '$' {
      i += 1;
      let (math, adv) = read_until_single_dollar(&chars, i);
      i += adv;
      out.push_str(&render(MathInput::Latex(math.trim())));
      continue;
    }
    out.push(chars[i]);
    i += 1;
  }
  textwrap::wrap(out.trim(), WRAP_WIDTH)
    .into_iter()
    .map(|l| l.to_string())
    .collect()
}
