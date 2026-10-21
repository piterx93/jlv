//! Finding the keys of a JSON line and where their values are, without parsing or
//! allocating anything (PRD §6.2, §14).

use std::ops::Range;

#[cfg(test)]
mod tests;

/// One key of a JSON object line and where its value sits in the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field<'a> {
    /// The key as written between its quotes, escapes not decoded.
    pub key: &'a [u8],
    /// Where the value sits in the line. For a string that is the text between its
    /// quotes, escapes not decoded; any other value is covered whole: `42`, `null`,
    /// `[1,2]`, `{"a":1}`.
    pub value: Range<usize>,
    /// Whether the value is a string.
    pub string: bool,
}

/// The fields of the object on `line`, one level deep and in the order they are
/// written; a repeated key comes once per occurrence. Whitespace may come before the
/// object and between any of its tokens. A line that isn't an object yields nothing.
///
/// Nothing is rejected outright: at the first byte that can't continue the object,
/// or where the line ends inside a value, the fields read so far are all there is.
/// Bytes inside strings are not checked, so invalid UTF-8 there does no harm.
pub fn fields(line: &[u8]) -> Fields<'_> {
    let start = skip_whitespace(line, 0);
    // `at` rests on the byte before the first key, as it does after every field.
    let at = if line.get(start) == Some(&b'{') {
        start
    } else {
        line.len()
    };
    Fields { line, at }
}

/// The first field of the object on `line` whose key is written exactly as `key`.
pub fn find<'a>(line: &'a [u8], key: &[u8]) -> Option<Field<'a>> {
    fields(line).find(|field| field.key == key)
}

/// The fields of one line, see [`fields`].
#[derive(Debug, Clone)]
pub struct Fields<'a> {
    line: &'a [u8],
    /// The `{` or `,` that precedes the next field; the line's length once done.
    at: usize,
}

impl<'a> Iterator for Fields<'a> {
    type Item = Field<'a>;

    fn next(&mut self) -> Option<Field<'a>> {
        let field = self.read();
        if field.is_none() {
            self.at = self.line.len();
        }
        field
    }
}

impl<'a> Fields<'a> {
    fn read(&mut self) -> Option<Field<'a>> {
        let line = self.line;
        if self.at >= line.len() {
            return None;
        }
        let key_start = skip_whitespace(line, self.at + 1);
        if line.get(key_start) != Some(&b'"') {
            return None;
        }
        let key_end = string_end(line, key_start + 1)?;
        let colon = skip_whitespace(line, key_end + 1);
        if line.get(colon) != Some(&b':') {
            return None;
        }
        let value_start = skip_whitespace(line, colon + 1);
        let (value, string, value_end) = value_at(line, value_start)?;
        // The next byte must be the comma that continues the object, or the object
        // ends here.
        let after = skip_whitespace(line, value_end);
        self.at = if line.get(after) == Some(&b',') {
            after
        } else {
            line.len()
        };
        Some(Field {
            key: &line[key_start + 1..key_end],
            value,
            string,
        })
    }
}

fn skip_whitespace(line: &[u8], from: usize) -> usize {
    let rest = line.get(from..).unwrap_or_default();
    let blanks = rest
        .iter()
        .take_while(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        .count();
    from + blanks
}

/// Where the string whose first byte after the opening quote is `from` closes.
fn string_end(line: &[u8], from: usize) -> Option<usize> {
    let mut at = from;
    loop {
        let found = at + memchr::memchr2(b'"', b'\\', line.get(at..)?)?;
        if line[found] == b'"' {
            return Some(found);
        }
        at = found + 2;
    }
}

/// The value starting at `start`: its range, whether it is a string, and where the
/// value (quotes included) ends. `None` when the line ends inside it.
fn value_at(line: &[u8], start: usize) -> Option<(Range<usize>, bool, usize)> {
    match *line.get(start)? {
        b'"' => {
            let end = string_end(line, start + 1)?;
            Some((start + 1..end, true, end + 1))
        }
        b'{' | b'[' => {
            let end = container_end(line, start)?;
            Some((start..end, false, end))
        }
        _ => {
            let rest = &line[start..];
            let len = rest.iter().position(|byte| {
                matches!(byte, b',' | b'}' | b']' | b' ' | b'\t' | b'\r' | b'\n')
            })?;
            (len > 0).then_some((start..start + len, false, start + len))
        }
    }
}

/// The end of the object or array opening at `start`. Depth is counted rather than
/// recursed into, so no nesting can overflow the stack.
fn container_end(line: &[u8], start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut at = start;
    while let Some(&byte) = line.get(at) {
        match byte {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at + 1);
                }
            }
            b'"' => at = string_end(line, at + 1)?,
            _ => {}
        }
        at += 1;
    }
    None
}
