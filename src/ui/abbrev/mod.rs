//! Shortening logger names to fit a column: Logback-style for dotted class names,
//! by dropping leading segments for paths such as a Go caller.

use std::borrow::Cow;

use unicode_width::UnicodeWidthStr;

#[cfg(test)]
mod tests;

/// `name` shortened Logback-style to fit `width` terminal columns: package segments
/// are cut to their first character from the left, one at a time, until the whole
/// name, dots included, fits. The last segment, the class name with any `$Inner`
/// suffix, is never shortened. If even the fully abbreviated name is too wide it is
/// returned anyway and the column truncates it, so unlike Logback's `%logger{0}`,
/// width 0 still keeps the packages' initials.
///
/// Width is measured in terminal columns, so a wide (CJK) character counts as two.
/// Empty segments, from leading, trailing or doubled dots, stay empty. Borrows `name`
/// when nothing changes.
///
/// `com.mycompany.pay.PayClient` at width 18 is `c.m.pay.PayClient`, at 15 or less
/// `c.m.p.PayClient`.
///
/// A name with a `/` before any `.` and a `.` after it is a path, such as the caller
/// `internal/orders/service.go:42` of a Go logger. Its dots and colons belong to the
/// file name, so leading segments are dropped instead, one at a time, until the rest
/// fits. The last segment is always kept. A `/` among package names, as in Tomcat's
/// `[/]`, or in a lambda's `Main$$Lambda$14/0x01` (Spring cuts the package off long
/// names), doesn't make a path.
pub fn abbreviate(name: &str, width: usize) -> Cow<'_, str> {
    if is_path(name) {
        return Cow::Borrowed(drop_leading_segments(name, width));
    }
    let Some((packages, _)) = name.rsplit_once('.') else {
        return Cow::Borrowed(name);
    };

    // Measuring once and subtracting what each cut saves keeps this linear: it runs
    // for every visible row on every frame.
    let mut remaining = name.width();
    let mut shortened = 0;
    for (i, segment) in packages.split('.').enumerate() {
        if remaining <= width {
            break;
        }
        let first = first_char(segment);
        if first.len() < segment.len() {
            // Saturating because unicode-width measures some sequences, such as an
            // emoji with a text variation selector, narrower than their first char.
            let saving = segment.width().saturating_sub(first.width());
            remaining = remaining.saturating_sub(saving);
            shortened = i + 1;
        }
    }
    if shortened == 0 {
        return Cow::Borrowed(name);
    }

    let mut abbreviated = String::with_capacity(name.len());
    for (i, segment) in name.split('.').enumerate() {
        if i > 0 {
            abbreviated.push('.');
        }
        let kept = if i < shortened {
            first_char(segment)
        } else {
            segment
        };
        abbreviated.push_str(kept);
    }
    Cow::Owned(abbreviated)
}

fn is_path(name: &str) -> bool {
    name.split_once('/')
        .is_some_and(|(head, tail)| !head.contains('.') && tail.contains('.'))
}

fn drop_leading_segments(path: &str, width: usize) -> &str {
    let mut rest = path;
    while rest.width() > width {
        match rest.split_once('/') {
            Some((_, tail)) => rest = tail,
            None => break,
        }
    }
    rest
}

fn first_char(segment: &str) -> &str {
    let len = segment.chars().next().map_or(0, char::len_utf8);
    &segment[..len]
}
