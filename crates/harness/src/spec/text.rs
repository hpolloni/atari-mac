//! Text helpers: IDs, markdown sections, table cells, code spans and links.

use std::collections::BTreeSet;

use super::Section;

/// The leading ASCII digits of `text`.
pub(super) fn digits(text: &str) -> &str {
    let end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    text.get(..end).unwrap_or_default()
}

/// Each reference `{prefix}{digits}` in `text` (for example `REQ-001`), with its byte offset.
/// The characters before the prefix and after the digits must not be letters or digits, so
/// a family pattern such as `REQ-2xx` is not a reference.
pub(super) fn references<'a>(text: &'a str, prefix: &str) -> Vec<(usize, &'a str)> {
    let mut found = Vec::new();
    for (start, _) in text.match_indices(prefix) {
        let before = text.get(..start).and_then(|t| t.chars().next_back());
        if before.is_some_and(char::is_alphanumeric) {
            continue;
        }
        let rest = text.get(start + prefix.len()..).unwrap_or_default();
        let number = digits(rest);
        let after = rest.get(number.len()..).and_then(|t| t.chars().next());
        if !number.is_empty()
            && !after.is_some_and(char::is_alphanumeric)
            && let Some(token) = text.get(start..start + prefix.len() + number.len())
        {
            found.push((start, token));
        }
    }
    found
}

/// Parses an exact ID such as `REQ-001` (`prefix` `REQ-`, `width` 3).
pub(super) fn parse_id(text: &str, prefix: &str, width: usize) -> Option<u16> {
    let number = text.strip_prefix(prefix)?;
    (number.len() == width && digits(number) == number)
        .then(|| number.parse().ok())
        .flatten()
}

/// All requirement numbers in `text`. A range `REQ-NNN to REQ-NNN` gives each number in it.
#[must_use]
pub fn requirement_numbers(text: &str) -> BTreeSet<u16> {
    let found = references(text, "REQ-");
    let mut numbers = BTreeSet::new();
    for (index, &(start, token)) in found.iter().enumerate() {
        let Ok(first) = token.trim_start_matches("REQ-").parse::<u16>() else {
            continue;
        };
        let _new = numbers.insert(first);
        let after = text.get(start + token.len()..).unwrap_or_default();
        if let Some(&(_, next)) = found.get(index + 1)
            && after.starts_with(" to REQ-")
            && let Ok(last) = next.trim_start_matches("REQ-").parse::<u16>()
        {
            numbers.extend(first..=last);
        }
    }
    numbers
}

/// The contents of the code spans (text in backticks) of one line.
pub(super) fn code_spans(line: &str) -> Vec<&str> {
    line.split('`').skip(1).step_by(2).collect()
}

/// `line` without its code spans. Each span becomes one space.
pub(super) fn without_code(line: &str) -> String {
    line.split('`').step_by(2).collect::<Vec<_>>().join(" ")
}

/// The cells of a markdown table row. A `|` in a code span does not split a cell.
pub(super) fn cells(line: &str) -> Vec<String> {
    let inner = line.trim().trim_start_matches('|').trim_end_matches('|');
    let mut out = vec![String::new()];
    let mut in_code = false;
    for c in inner.chars() {
        if c == '`' {
            in_code = !in_code;
        }
        if c == '|' && !in_code {
            out.push(String::new());
        } else if let Some(cell) = out.last_mut() {
            cell.push(c);
        }
    }
    out.into_iter().map(|cell| cell.trim().to_owned()).collect()
}

/// True if `line` is a table row (it starts with `|`) and not a separator row.
pub(super) fn is_table_row(line: &str) -> bool {
    let line = line.trim();
    line.starts_with('|') && !line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

/// True if `line` starts or ends a fenced code block.
pub(super) fn is_fence(line: &str) -> bool {
    line.trim_start().starts_with("```")
}

/// The level-2 sections (`## Title`) of a markdown body. `lines` holds the line numbers.
/// Headings in fenced code blocks do not count.
pub(super) fn sections(lines: &[(usize, &str)]) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut in_fence = false;
    for &(number, line) in lines {
        if is_fence(line) {
            in_fence = !in_fence;
        }
        if let Some(title) = line.strip_prefix("## ").filter(|_| !in_fence) {
            out.push(Section {
                title: title.trim().to_owned(),
                line: number,
                lines: Vec::new(),
            });
        } else if let Some(section) = out.last_mut() {
            section.lines.push((number, line.to_owned()));
        }
    }
    out
}

/// The targets of the inline markdown links (`[text](target)`) of one line, outside code
/// spans.
pub(super) fn links(line: &str) -> Vec<String> {
    let text = without_code(line);
    let mut out = Vec::new();
    for (start, _) in text.match_indices("](") {
        let rest = text.get(start + 2..).unwrap_or_default();
        let Some(end) = rest.find(')') else {
            continue;
        };
        let target = rest.get(..end).unwrap_or_default();
        let target = target.split_whitespace().next().unwrap_or_default();
        if !target.is_empty() {
            out.push(target.to_owned());
        }
    }
    out
}

/// True if `c` is part of a word: a letter, a digit, `-` or `_`.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

/// True if `text` contains `phrase` as a whole word, without regard to case.
pub(super) fn contains_word(text: &str, phrase: &str) -> bool {
    let text = text.to_lowercase();
    let phrase = phrase.to_lowercase();
    text.match_indices(&phrase).any(|(start, _)| {
        let before = text.get(..start).and_then(|t| t.chars().next_back());
        let after = text
            .get(start + phrase.len()..)
            .and_then(|t| t.chars().next());
        !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
    })
}

/// True if `text` is a snake-case Rust identifier with at least one `_`, for example
/// `boots_to_basic_ready`. The test plans name tests in this form.
pub(super) fn is_test_name(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && text.contains('_')
}

/// The number of a numbered list item (`3. text` gives 3).
pub(super) fn list_number(line: &str) -> Option<usize> {
    let number = digits(line);
    line.get(number.len()..)
        .is_some_and(|rest| rest.starts_with(". "))
        .then(|| number.parse().ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies REQ-043: an ID has exactly the width of its digits.
    #[test]
    fn ids_have_an_exact_width() {
        assert_eq!(parse_id("REQ-012", "REQ-", 3), Some(12));
        assert_eq!(parse_id(concat!("REQ", "-0012"), "REQ-", 3), None);
        assert_eq!(parse_id("REQ-01a", "REQ-", 3), None);
        assert_eq!(parse_id("OI-012", "REQ-", 3), None);
    }

    /// Verifies REQ-048: a `|` in a code span does not split a table cell, and a separator row
    /// or a line without a leading `|` is not a table row.
    #[test]
    fn table_rows_and_cells() {
        assert_eq!(cells("| `a|b` | c |"), ["`a|b`", "c"]);
        assert_eq!(cells("| a | b |"), ["a", "b"]);
        assert!(is_table_row("| a | b |"));
        assert!(!is_table_row("| --- | :-: |"));
        assert!(!is_table_row("text | a |"));
    }

    /// Verifies REQ-044: a `## ` heading in a fenced code block is not a section.
    #[test]
    fn headings_in_code_blocks_are_not_sections() {
        let lines = [
            (1, "## One"),
            (2, "```"),
            (3, "## Not a section"),
            (4, "```"),
            (5, "## Two"),
        ];
        let titles: Vec<String> = sections(&lines).into_iter().map(|s| s.title).collect();
        assert_eq!(titles, ["One", "Two"]);
    }

    /// Verifies REQ-045: a phrase matches only as a whole word: a letter, a digit, `-` or `_`
    /// next to it is part of the word.
    #[test]
    fn whole_words_only() {
        assert!(contains_word("It is Correct.", "correct"));
        for text in [
            "incorrect",
            "correctly",
            "a correct_b",
            "a_correct",
            "a-correct",
            "xx correctness",
        ] {
            assert!(!contains_word(text, "correct"), "{text}");
        }
    }

    /// Verifies REQ-049: a test name in a test plan is a snake-case identifier with a `_`.
    #[test]
    fn test_names_are_snake_case_with_an_underscore() {
        assert!(is_test_name("boots_to_basic"));
        assert!(is_test_name("_a_b"));
        for text in ["abc", "1a_b", "A_b", "a_B", "a-b_c"] {
            assert!(!is_test_name(text), "{text}");
        }
    }
}
