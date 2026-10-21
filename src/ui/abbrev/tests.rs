use std::iter;

use proptest::prelude::*;
use unicode_width::UnicodeWidthStr;

use super::*;

const PAY_CLIENT: &str = "com.mycompany.pay.PayClient";

fn abbreviated(name: &str, width: usize) -> String {
    abbreviate(name, width).into_owned()
}

#[test]
fn leaves_a_name_that_fits_exactly_unchanged() {
    assert_eq!(abbreviated(PAY_CLIENT, 27), PAY_CLIENT);
}

#[test]
fn abbreviates_the_leftmost_package_first() {
    assert_eq!(abbreviated(PAY_CLIENT, 26), "c.mycompany.pay.PayClient");
}

#[test]
fn abbreviates_packages_from_the_left_until_the_name_fits() {
    assert_eq!(abbreviated(PAY_CLIENT, 18), "c.m.pay.PayClient");
}

#[test]
fn abbreviates_every_package_when_needed() {
    assert_eq!(abbreviated(PAY_CLIENT, 15), "c.m.p.PayClient");
}

#[test]
fn returns_the_fully_abbreviated_name_when_even_that_does_not_fit() {
    assert_eq!(abbreviated(PAY_CLIENT, 5), "c.m.p.PayClient");
}

#[test]
fn width_zero_gives_the_fully_abbreviated_name() {
    assert_eq!(abbreviated(PAY_CLIENT, 0), "c.m.p.PayClient");
}

#[test]
fn never_shortens_a_name_without_dots() {
    let result = abbreviate("PayClient", 3);
    assert_eq!(result, "PayClient");
    assert!(matches!(result, Cow::Borrowed(_)));
}

#[test]
fn an_empty_name_stays_empty() {
    let result = abbreviate("", 0);
    assert_eq!(result, "");
    assert!(matches!(result, Cow::Borrowed(_)));
}

#[test]
fn abbreviates_past_an_empty_first_segment() {
    assert_eq!(
        abbreviated(".com.mycompany.PayClient", 20),
        ".c.m.PayClient"
    );
}

#[test]
fn a_trailing_dot_makes_the_kept_last_segment_empty() {
    assert_eq!(abbreviated("com.mycompany.", 5), "c.m.");
}

#[test]
fn abbreviates_past_an_empty_segment_from_a_doubled_dot() {
    assert_eq!(
        abbreviated("com..mycompany.PayClient", 20),
        "c..m.PayClient"
    );
}

#[test]
fn leaves_a_name_of_only_dots_unchanged() {
    assert_eq!(abbreviated("...", 1), "...");
}

#[test]
fn keeps_an_inner_class_with_its_outer_class() {
    assert_eq!(
        abbreviated("com.mycompany.pay.PayClient$Retry", 10),
        "c.m.p.PayClient$Retry"
    );
}

#[test]
fn never_shortens_an_inner_class_without_a_package() {
    assert_eq!(abbreviated("PayClient$Retry", 5), "PayClient$Retry");
}

#[test]
fn leaves_an_already_abbreviated_name_unchanged() {
    let result = abbreviate("o.s.b.SpringApplication", 10);
    assert_eq!(result, "o.s.b.SpringApplication");
    assert!(matches!(result, Cow::Borrowed(_)));
}

#[test]
fn skips_single_character_packages_on_the_way_to_a_longer_one() {
    assert_eq!(
        abbreviated("o.s.boot.SpringApplication", 25),
        "o.s.b.SpringApplication"
    );
}

#[test]
fn treats_wildfly_categories_like_class_names() {
    assert_eq!(abbreviated("org.jboss.as.server", 13), "o.j.as.server");
}

#[test]
fn abbreviates_every_package_of_a_wildfly_category_when_needed() {
    assert_eq!(abbreviated("org.jboss.as.server", 12), "o.j.a.server");
}

/// A Go caller as zap writes it: 29 columns.
const GO_CALLER: &str = "internal/orders/service.go:42";

#[test]
fn drops_leading_path_segments_until_a_path_fits() {
    assert_eq!(abbreviated(GO_CALLER, 20), "orders/service.go:42");
}

/// A name is a path when it has a `/`, no `.` before its first `/` and a `.` after
/// it. Neither a lambda that Spring cut to 40 columns from the left nor Tomcat's
/// dotted logger with a `[/]` further on is one.
#[test]
fn only_a_slash_between_dotless_segments_and_a_dot_makes_a_path() {
    let lambda = "erService$$Lambda$1342/0x0000000801c2a38";
    assert_eq!(abbreviated(lambda, 10), lambda);
    assert_eq!(
        abbreviated("o.a.catalina.core.C.[Tomcat].[/]", 30),
        "o.a.c.core.C.[Tomcat].[/]"
    );
}

/// 15 characters, but 24 bytes in UTF-8.
const POLISH: &str = "łódź.zażółć.Gęś";

#[test]
fn counts_characters_not_bytes() {
    assert_eq!(abbreviated(POLISH, 15), POLISH);
}

#[test]
fn abbreviates_a_package_to_its_first_character_not_its_first_byte() {
    assert_eq!(abbreviated(POLISH, 14), "ł.zażółć.Gęś");
}

/// 11 characters, 15 terminal columns: each CJK character takes two.
const WIDE: &str = "日志.服务.Klasa";

#[test]
fn counts_wide_characters_as_two_columns() {
    assert_eq!(abbreviated(WIDE, 13), "日.服务.Klasa");
}

#[test]
fn abbreviates_wide_packages_until_the_columns_fit() {
    assert_eq!(abbreviated(WIDE, 12), "日.服.Klasa");
}

/// Characters class names are made of: ASCII identifier characters, `$`, Latin
/// letters beyond ASCII and CJK characters two columns wide.
const ALPHABET: &str = "abcxyzABCXYZ019_$łóżéß日志服务";

fn segment() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(ALPHABET.chars().collect::<Vec<_>>()),
        0..=8,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

/// Dotted names of one to six segments, any of which may be empty.
fn class_name() -> impl Strategy<Value = String> {
    prop::collection::vec(segment(), 1..=6).prop_map(|segments| segments.join("."))
}

/// A path segment of up to 8 characters drawn from `alphabet`.
fn path_segment(alphabet: &str) -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(alphabet.chars().collect::<Vec<_>>()),
        0..=8,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

/// Go callers of two to six segments, any of which may be empty but the file: a
/// first segment without dots, segments holding dots, colons and digits, and a Go
/// file with its line, such as `service.go:42`, last.
fn path_name() -> impl Strategy<Value = String> {
    let first = path_segment("abxyzAB019_-:łó日志");
    let middle = prop::collection::vec(path_segment("abxyzAB019_-.:łó日志"), 0..=4);
    let file = (path_segment("abxyz_日"), any::<u16>())
        .prop_map(|(name, line)| format!("{name}.go:{line}"));
    (first, middle, file)
        .prop_map(|(first, middle, file)| [vec![first], middle, vec![file]].concat().join("/"))
}

/// Whether `name` is a path: it has a `/`, no `.` before the first one and a `.`
/// after it.
fn is_path(name: &str) -> bool {
    name.split_once('/')
        .is_some_and(|(head, tail)| !head.contains('.') && tail.contains('.'))
}

/// The part of `name` that is never shortened: what follows the last `/` of a path,
/// or else the last `.`.
fn last_segment(name: &str) -> &str {
    let split = if is_path(name) {
        name.rsplit_once('/')
    } else {
        name.rsplit_once('.')
    };
    split.map_or(name, |(_, last)| last)
}

/// The answer for a path worked out the slow way: the whole path, then the path
/// without its first segment, without its first two and so on, and the first of
/// those that fits, or the last segment alone if none does.
fn dropping_one_segment_at_a_time(name: &str, width: usize) -> String {
    let starts = iter::once(0).chain(name.match_indices('/').map(|(at, _)| at + 1));
    let candidates: Vec<&str> = starts.map(|start| &name[start..]).collect();
    let fitting = candidates
        .iter()
        .find(|candidate| candidate.width() <= width);
    fitting.or(candidates.last()).unwrap().to_string()
}

/// The answer worked out the slow way: abbreviate no package, then the first, then
/// the first two and so on, and take the first of those names that fits, or the last
/// one if none does.
fn one_package_at_a_time(name: &str, width: usize) -> String {
    let segments: Vec<&str> = name.split('.').collect();
    let mut candidate = name.to_string();
    for cut in 0..segments.len() {
        candidate = segments
            .iter()
            .enumerate()
            .map(|(i, segment)| {
                if i < cut {
                    segment.chars().take(1).collect()
                } else {
                    segment.to_string()
                }
            })
            .collect::<Vec<String>>()
            .join(".");
        if candidate.width() <= width {
            break;
        }
    }
    candidate
}

proptest! {
    #[test]
    fn never_has_more_characters_than_the_input(
        name in any::<String>(),
        width in 0usize..64,
    ) {
        let result = abbreviate(&name, width);
        prop_assert!(result.chars().count() <= name.chars().count());
    }

    #[test]
    fn keeps_the_last_segment(name in any::<String>(), width in any::<usize>()) {
        let result = abbreviate(&name, width);
        prop_assert_eq!(last_segment(&result), last_segment(&name));
    }

    #[test]
    fn is_never_wider_than_the_input(name in class_name(), width in 0usize..64) {
        prop_assert!(abbreviate(&name, width).width() <= name.width());
    }

    #[test]
    fn is_idempotent(name in class_name(), width in 0usize..64) {
        let once = abbreviate(&name, width).into_owned();
        prop_assert_eq!(abbreviate(&once, width), once.as_str());
    }

    #[test]
    fn returns_a_name_that_fits_as_is(name in class_name(), spare in 0usize..8) {
        let width = name.width() + spare;
        prop_assert!(matches!(abbreviate(&name, width), Cow::Borrowed(n) if n == name));
    }

    #[test]
    fn abbreviates_one_package_at_a_time_from_the_left(
        name in class_name(),
        width in 0usize..64,
    ) {
        prop_assert_eq!(abbreviate(&name, width), one_package_at_a_time(&name, width));
    }

    #[test]
    fn drops_leading_path_segments_one_at_a_time(name in path_name(), width in 0usize..64) {
        prop_assert_eq!(abbreviate(&name, width), dropping_one_segment_at_a_time(&name, width));
    }
}
