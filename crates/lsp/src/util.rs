use std::path::PathBuf;

use tower_lsp::lsp_types::{Position, TextEdit, WorkspaceEdit};

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

pub fn new_workspace_edit(uri: tower_lsp::lsp_types::Url, items: Vec<TextEdit>) -> WorkspaceEdit {
    WorkspaceEdit {
        changes: Some(vec![(uri, items)].into_iter().collect()),
        ..WorkspaceEdit::default()
    }
}

pub fn remove_file_prefix(path_buf: &PathBuf) -> Option<PathBuf> {
    let path_str = path_buf.to_str()?;

    let trimmed_path_str = if path_str.starts_with("file://") {
        &path_str[7..]
    } else {
        path_str
    };

    Some(PathBuf::from(trimmed_path_str))
}
