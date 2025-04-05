use std::fmt::Display;

use difference::Difference;
use taplo::formatter::Options;

use crate::{tree::RangeExclusive, Db, Indent, Uri};

impl Display for Indent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Indent::Spaces(count) => write!(f, "{}", vec![" "; *count as usize].join("")),
            Indent::Tab => write!(f, "\t"),
        }
    }
}

impl Db {
    pub fn format(
        &self,
        uri: &Uri,
        sort: bool,
        trailing_new_line: bool,
        indent: Indent,
    ) -> Option<Vec<(((usize, usize), (usize, usize)), String)>> {
        let content = self.get_content(uri)?;
        let crlf = content.contains("\r\n");
        let new = taplo::formatter::format(
            &content,
            Options {
                // align_entries: todo!(),
                // align_comments: todo!(),
                // align_single_comments: todo!(),
                // array_trailing_comma: true,
                // array_auto_expand: true,
                // inline_table_expand: todo!(),
                // array_auto_collapse: true,
                // compact_arrays: todo!(),
                // compact_inline_tables: todo!(),
                // compact_entries: todo!(),
                // column_width: todo!(),
                // indent_tables: todo!(),
                // indent_entries: todo!(),
                indent_string: indent.to_string(),
                trailing_newline: trailing_new_line,
                reorder_keys: sort,
                reorder_arrays: sort,
                allowed_blank_lines: 2,
                crlf,
                ..Default::default()
            },
        );

        let changeset = difference::Changeset::new(&content, &new, "");
        let mut differences = Vec::new();
        let mut left_offset_bytes = 0;

        for diff in changeset.diffs {
            match diff {
                Difference::Same(ref s) => {
                    left_offset_bytes += s.len();
                }
                Difference::Add(ref s) => {
                    differences.push((
                        RangeExclusive {
                            start: left_offset_bytes as u32,
                            end: left_offset_bytes as u32,
                        },
                        s.to_string(),
                    ));
                }
                Difference::Rem(ref s) => {
                    differences.push((
                        RangeExclusive {
                            start: left_offset_bytes as u32,
                            end: (left_offset_bytes + s.len()) as u32,
                        },
                        String::new(),
                    ));
                    left_offset_bytes += s.len();
                }
            }
        }

        let pos = self.get_last_line_and_char(uri).unwrap_or((0, 0));

        Some(vec![(((0, 0), pos), new)])
    }
}
