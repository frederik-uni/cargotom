use super::structure::{Positioned, RangeExclusive};

pub fn str_to_positioned(str: &str, range: &RangeExclusive) -> Positioned<String> {
    Positioned {
        start: range.start,
        end: range.end,
        data: str.to_string(),
    }
}
