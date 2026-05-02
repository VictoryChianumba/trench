#[derive(Debug, Clone, Copy, PartialEq, Eq, Default,
         serde::Serialize, serde::Deserialize)]
pub enum QueryIntent {
  #[default]
  FindPapers,
  LiteratureReview,
  SotaLookup,
  ReadingList,
  CodeSearch,
}

impl QueryIntent {
  pub fn label(self) -> &'static str {
    match self {
      Self::FindPapers       => "papers",
      Self::LiteratureReview => "lit review",
      Self::SotaLookup       => "sota",
      Self::ReadingList      => "reading list",
      Self::CodeSearch       => "code",
    }
  }
}

pub fn classify(topic: &str) -> QueryIntent {
  let t = topic.to_lowercase();

  if t.contains("benchmark") || t.contains("state of the art")
    || t.contains("sota") || t.contains("best model")
    || t.contains("leaderboard") || t.contains("beats ")
    || t.contains("performance on") || t.contains("score on")
  {
    return QueryIntent::SotaLookup;
  }

  if t.contains("survey") || t.contains("overview of")
    || t.contains("review of") || t.contains("state of")
    || t.contains("landscape") || t.contains("comprehensive")
    || t.contains("what is known")
  {
    return QueryIntent::LiteratureReview;
  }

  if t.contains("how to learn") || t.starts_with("learn ")
    || t.contains("getting started") || t.contains("beginner")
    || t.contains("roadmap") || t.contains("curriculum")
    || t.contains("reading list") || t.contains("from scratch")
  {
    return QueryIntent::ReadingList;
  }

  if t.contains("implementation") || t.contains("code for")
    || t.contains("github") || t.contains(" library")
    || t.contains("pytorch") || t.contains("tensorflow")
    || t.contains("how to implement")
  {
    return QueryIntent::CodeSearch;
  }

  QueryIntent::FindPapers
}
