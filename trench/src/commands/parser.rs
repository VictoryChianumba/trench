use crate::commands::registry::{COMMAND_SPECS, CommandId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashCommandInvocation {
  ClearChat,
  Discover { topic: String },
  ClearDiscoveries,
  AddArxivCategory { category: String },
  AddFeed { url: String },
  Trending { topic: String },
  Watch { topic: String },
  Unknown { raw: String },
}

pub fn parse_slash_command(raw: &str) -> SlashCommandInvocation {
  let trimmed = raw.trim();

  match find_command(trimmed).map(|spec| spec.id) {
    Some(CommandId::ClearChat) => SlashCommandInvocation::ClearChat,
    Some(CommandId::Discover) => SlashCommandInvocation::Discover {
      topic: trimmed.strip_prefix("/discover").unwrap_or("").trim().to_string(),
    },
    Some(CommandId::ClearDiscoveries) => {
      SlashCommandInvocation::ClearDiscoveries
    }
    Some(CommandId::AddArxivCategory) => {
      SlashCommandInvocation::AddArxivCategory {
        category: trimmed.strip_prefix("/add").unwrap_or("").trim().to_string(),
      }
    }
    Some(CommandId::AddFeed) => SlashCommandInvocation::AddFeed {
      url: trimmed.strip_prefix("/add-feed").unwrap_or("").trim().to_string(),
    },
    Some(CommandId::Trending) => SlashCommandInvocation::Trending {
      topic: trimmed.strip_prefix("/trending").unwrap_or("").trim().to_string(),
    },
    Some(CommandId::Watch) => SlashCommandInvocation::Watch {
      topic: trimmed.strip_prefix("/watch").unwrap_or("").trim().to_string(),
    },
    None => SlashCommandInvocation::Unknown { raw: trimmed.to_string() },
  }
}

fn find_command(
  raw: &str,
) -> Option<&'static crate::commands::registry::CommandSpec> {
  COMMAND_SPECS.iter().find(|spec| {
    raw == spec.command
      || raw
        .strip_prefix(spec.command)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
  })
}
