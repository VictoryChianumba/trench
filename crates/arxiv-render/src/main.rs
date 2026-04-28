use arxiv_render::{fetch, parse};

fn main() {
  let arg = std::env::args().nth(1).unwrap_or_default();
  if arg.is_empty() {
    eprintln!("usage: arxiv-render <arxiv-id-or-url>");
    std::process::exit(1);
  }

  let id = match fetch::extract_id(&arg) {
    Some(id) => id,
    None => {
      eprintln!("error: could not extract a valid arXiv ID from {:?}", arg);
      std::process::exit(1);
    }
  };

  eprintln!("fetching source for arXiv:{id} ...");

  let sources = match fetch::fetch_source(&id) {
    Ok(s) => s,
    Err(e) => {
      eprintln!("error: {e}");
      std::process::exit(1);
    }
  };

  eprintln!("found {} .tex file(s); parsing ...", sources.len());

  let lines = parse::to_lines(sources);

  for line in &lines {
    println!("{line}");
  }

  eprintln!("--- {} lines total ---", lines.len());
}
