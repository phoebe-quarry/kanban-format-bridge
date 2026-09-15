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
            let mark = if done_list { "x" } else { " " };
            out.push_str(&format!("- [{}] {}\n", mark, card_name));
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

    for raw_line in input.lines() {
        let line = raw_line.trim_end();
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
            card_count += 1;
            cards.push(Json::Object(vec![
                ("id".to_string(), Json::String(format!("card-{}", card_count))),
                ("name".to_string(), Json::String(rest.trim().to_string())),
                ("idList".to_string(), Json::String(list_id)),
                ("closed".to_string(), Json::Bool(false)),
            ]));
        }
    }

    let doc = Json::Object(vec![
        ("name".to_string(), Json::String(board_name)),
        ("lists".to_string(), Json::Array(lists)),
        ("cards".to_string(), Json::Array(cards)),
    ]);

    Ok(doc.to_pretty_string() + "\n")
}
