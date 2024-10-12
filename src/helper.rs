use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::lsp_types::{Position, TextEdit, WorkspaceEdit};

use crate::crate_lookup::Shared;

pub fn new_workspace_edit(uri: tower_lsp::lsp_types::Url, items: Vec<TextEdit>) -> WorkspaceEdit {
    WorkspaceEdit {
        changes: Some(vec![(uri, items)].into_iter().collect()),
        ..WorkspaceEdit::default()
    }
}

pub fn shared<T>(t: T) -> Shared<T> {
    Arc::new(RwLock::new(t))
}

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn get_byte_index_from_position(s: &str, position: Position) -> usize {
    if s.is_empty() {
        return 0;
    }
    let line_start = index_of_first_char_in_line(s, position.line).unwrap_or(s.len());

    let char_index = line_start + position.character as usize;

    if char_index >= s.len() {
        s.char_indices().nth(s.len() - 1).unwrap().0
    } else {
        s.char_indices().nth(char_index).unwrap().0
    }
}

fn index_of_first_char_in_line(s: &str, line: u32) -> Option<usize> {
    let mut current_line = 0;
    let mut index = 0;

    if line == 0 {
        return Some(0);
    }

    for (i, c) in s.char_indices() {
        if c == '\n' {
            current_line += 1;
            if current_line == line {
                return Some(i + 1);
            }
        }
        index = i;
    }

    if current_line == line - 1 {
        return Some(index + 1);
    }

    None
}
