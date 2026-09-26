mod json;

use json::Json;
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: {} <to-md|to-trello> [file]", program_name(&args));
        eprintln!("       (omit [file] or pass '-' to read from stdin)");
        return ExitCode::FAILURE;
    }

    let command = args[1].as_str();
    let path = args.get(2).map(|s| s.as_str());

    let input = match read_input(path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("error reading input: {}", err);
            return ExitCode::FAILURE;
        }
    };

    let result = match command {
        "to-md" => trello_to_markdown(&input),
        "to-trello" => markdown_to_trello(&input),
        other => {
            eprintln!("unknown command '{}', expected 'to-md' or 'to-trello'", other);
            return ExitCode::FAILURE;
        }
    };

    match result {
        Ok(output) => {
            print!("{}", output);
            let _ = io::stdout().flush();
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {}", err);
            ExitCode::FAILURE
        }
    }
}

fn program_name(args: &[String]) -> &str {
    args.first()
        .and_then(|p| p.rsplit('/').next())
        .unwrap_or("kanban-bridge")
}

fn read_input(path: Option<&str>) -> io::Result<String> {
    match path {
        Some(p) if p != "-" => fs::read_to_string(p),
        _ => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            Ok(buf)
        }
    }
}

// Trello's export puts cards and lists in separate flat arrays linked by id,
// which reads badly as a checklist. This walks the lists in export order and
// pulls in each list's own cards, skipping anything archived ("closed").
fn trello_to_markdown(input: &str) -> Result<String, String> {
    let doc = Json::parse(input)?;
    let board_name = doc.get("name").and_then(Json::as_str).unwrap_or("Board");
    let lists = doc
        .get("lists")
        .and_then(Json::as_array)
        .ok_or_else(|| "missing 'lists' array in trello export".to_string())?;
    let cards = doc
        .get("cards")
        .and_then(Json::as_array)
        .ok_or_else(|| "missing 'cards' array in trello export".to_string())?;

    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", board_name));

    for list in lists {
        if list.get("closed").and_then(Json::as_bool).unwrap_or(false) {
            continue;
        }
        let list_id = list.get("id").and_then(Json::as_str).unwrap_or("");
        let list_name = list.get("name").and_then(Json::as_str).unwrap_or("Untitled");
        out.push_str(&format!("## {}\n\n", list_name));

        // Trello has no notion of a checked-off card, so a list named
        // "Done" (any case) is treated as the finished column.
        let done_list = list_name.to_lowercase().contains("done");

        for card in cards {
            if card.get("closed").and_then(Json::as_bool).unwrap_or(false) {
                continue;
            }
            if card.get("idList").and_then(Json::as_str) != Some(list_id) {
                continue;
            }
            let card_name = card.get("name").and_then(Json::as_str).unwrap_or("Untitled");
            let due = card.get("due").and_then(Json::as_str).filter(|s| !s.is_empty());
            let mark = if done_list { "x" } else { " " };
            match due {
                Some(d) => out.push_str(&format!("- [{}] {} (due {})\n", mark, card_name, d)),
                None => out.push_str(&format!("- [{}] {}\n", mark, card_name)),
            }

            // The description becomes an indented block right under the card:
            // blank lines inside it stay blank so paragraph breaks survive,
            // everything else gets the two-space indent that marks it as
            // belonging to this card rather than being a new list item.
            let desc = card.get("desc").and_then(Json::as_str).map(str::trim).filter(|s| !s.is_empty());
            if let Some(desc) = desc {
                for line in desc.lines() {
                    if line.is_empty() {
                        out.push('\n');
                    } else {
                        out.push_str("  ");
                        out.push_str(line);
                        out.push('\n');
                    }
                }
            }
        }
        out.push('\n');
    }

    Ok(out)
}

// Reads a "# board" / "## list" / "- [ ] card" checklist and rebuilds the
// minimum shape of a Trello export: a board name plus lists and cards linked
// by generated ids.
fn markdown_to_trello(input: &str) -> Result<String, String> {
    let mut board_name = "Board".to_string();
    let mut have_board_name = false;
    let mut lists: Vec<Json> = Vec::new();
    let mut cards: Vec<Json> = Vec::new();
    let mut current_list_id: Option<String> = None;
    let mut list_count = 0;
    let mut card_count = 0;
    let mut desc_lines: Vec<String> = Vec::new();
    let mut pending_blanks = 0usize;

    for raw_line in input.lines() {
        let line = raw_line.trim_end();

        // A two-space-indented line right after a card is part of its
        // description. Blank lines inside that block are held back until we
        // know whether more description text follows, so paragraph breaks
        // round-trip but a blank line that just separates cards doesn't.
        if !cards.is_empty() && line.starts_with("  ") && !line.trim().is_empty() {
            desc_lines.extend(std::iter::repeat(String::new()).take(pending_blanks));
            pending_blanks = 0;
            desc_lines.push(line[2..].to_string());
            continue;
        }
        if line.is_empty() && !desc_lines.is_empty() {
            pending_blanks += 1;
            continue;
        }
        flush_desc(&mut cards, &mut desc_lines);
        pending_blanks = 0;

        if let Some(rest) = line.strip_prefix("## ") {
            list_count += 1;
            let id = format!("list-{}", list_count);
            lists.push(Json::Object(vec![
                ("id".to_string(), Json::String(id.clone())),
                ("name".to_string(), Json::String(rest.trim().to_string())),
                ("closed".to_string(), Json::Bool(false)),
            ]));
            current_list_id = Some(id);
        } else if let Some(rest) = line.strip_prefix("# ") {
            if !have_board_name {
                board_name = rest.trim().to_string();
                have_board_name = true;
            }
        } else if let Some(rest) = line
            .strip_prefix("- [ ] ")
            .or_else(|| line.strip_prefix("- [x] "))
        {
            let list_id = current_list_id
                .clone()
                .ok_or_else(|| "found a card before any list heading ('## ...')".to_string())?;
            let (name, due) = split_due(rest.trim());
            card_count += 1;
            let mut fields = vec![
                ("id".to_string(), Json::String(format!("card-{}", card_count))),
                ("name".to_string(), Json::String(name)),
                ("idList".to_string(), Json::String(list_id)),
                ("closed".to_string(), Json::Bool(false)),
            ];
            if let Some(due) = due {
                fields.push(("due".to_string(), Json::String(due)));
            }
            cards.push(Json::Object(fields));
        }
    }
    flush_desc(&mut cards, &mut desc_lines);

    let doc = Json::Object(vec![
        ("name".to_string(), Json::String(board_name)),
        ("lists".to_string(), Json::Array(lists)),
        ("cards".to_string(), Json::Array(cards)),
    ]);

    Ok(doc.to_pretty_string() + "\n")
}

// Card lines can end with "(due <value>)"; the due value is a Trello
// timestamp, which never contains a space, so that's the signal used to tell
// it apart from a card name that happens to contain the word "due".
fn split_due(text: &str) -> (String, Option<String>) {
    if let Some(idx) = text.rfind(" (due ") {
        if let Some(due) = text.strip_suffix(')').map(|s| &s[idx + 6..]) {
            if !due.is_empty() && !due.contains(' ') {
                return (text[..idx].to_string(), Some(due.to_string()));
            }
        }
    }
    (text.to_string(), None)
}

// Attaches any buffered description lines to the most recently parsed card,
// trimming the trailing blank lines that pending_blanks may have queued up
// right before the block ended.
fn flush_desc(cards: &mut [Json], desc_lines: &mut Vec<String>) {
    while desc_lines.last().map(|s| s.is_empty()).unwrap_or(false) {
        desc_lines.pop();
    }
    if desc_lines.is_empty() {
        return;
    }
    if let Some(Json::Object(pairs)) = cards.last_mut() {
        pairs.push(("desc".to_string(), Json::String(desc_lines.join("\n"))));
    }
    desc_lines.clear();
}
