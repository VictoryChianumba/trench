use block_reader::PaperMeta;
use doc_model::Block;
use std::sync::mpsc;

pub fn spawn(
  id: String,
  title: String,
  authors: String,
  tx: mpsc::Sender<Result<(Vec<Block>, PaperMeta), String>>,
) {
  std::thread::spawn(move || {
    let sources = match arxiv_render::fetch::fetch_source(&id) {
      Ok(s) => s,
      Err(e) => {
        let _ = tx.send(Err(e.to_string()));
        return;
      }
    };
    let blocks = arxiv_render::parse::to_blocks(sources);
    let meta = PaperMeta { title, authors };
    let _ = tx.send(Ok((blocks, meta)));
  });
}
