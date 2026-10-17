//! The configuration file (PRD §11): `$XDG_CONFIG_HOME/vilog/config.toml`, or the
//! file `--config` names. Without the first, `$XDG_CONFIG_HOME/jlv/config.toml` is
//! read, where the file was before vilog had its name.

use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use jiff::civil;
use ratatui::style::{Color, Style};
use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};

use crate::app::Action;
use crate::filter::Classes;
use crate::format::Level;
use crate::keymap::{self, Key, Keymap};

#[cfg(test)]
mod tests;

/// Everything the configuration file sets. The default is what PRD §11 lists, and
/// what vilog uses without a file.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub ui: Ui,
    pub colors: Colors,
    pub follow: Follow,
    pub filters: Filters,
    /// The `[keys]` section: the default key bindings with the actions it names
    /// bound to the keys it gives them.
    pub keys: Keymap,
}

/// The file as written.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    ui: Ui,
    #[serde(default)]
    colors: Colors,
    #[serde(default)]
    follow: Follow,
    #[serde(default)]
    filters: Filters,
    #[serde(default, deserialize_with = "keys")]
    keys: Keymap,
}

/// The `[ui]` section.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ui {
    /// How the timestamp column shows a timestamp from 100 columns on, in strftime
    /// syntax; narrower screens show `%H:%M:%S%.3f`.
    #[serde(deserialize_with = "timestamp_format")]
    pub timestamp_format: String,
    pub show_thread: bool,
    pub show_histogram: bool,
    pub show_line_numbers: bool,
    pub mouse: bool,
    pub wrap: bool,
}

/// The `[colors]` section: the style of each level in the level column, and of the
/// line numbers in the gutter. A colour is written as a name, `#rrggbb` or an index
/// into the 256-colour palette (`"42"`), or as an
/// inline table `{ fg = "white", bg = "red", bold = true }` whose keys are all
/// optional.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Colors {
    #[serde(deserialize_with = "style")]
    pub trace: Style,
    #[serde(deserialize_with = "style")]
    pub debug: Style,
    #[serde(deserialize_with = "style")]
    pub info: Style,
    #[serde(deserialize_with = "style")]
    pub warn: Style,
    #[serde(deserialize_with = "style")]
    pub error: Style,
    #[serde(deserialize_with = "style")]
    pub fatal: Style,
    #[serde(deserialize_with = "style")]
    pub line_number: Style,
}

/// The `[follow]` section.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Follow {
    /// `poll_interval_ms`: how long the watcher waits between two looks at the file.
    #[serde(rename = "poll_interval_ms", deserialize_with = "poll_interval")]
    pub poll_interval: Duration,
}

/// The `[filters]` section.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Filters {
    /// `default_class_excludes`, one exclude rule per item: the class filter vilog
    /// starts with unless `--class` is given.
    #[serde(deserialize_with = "class_excludes")]
    pub default_class_excludes: Classes,
}

/// What a timestamp is shown with at most; `Ui::timestamp_format` is checked against
/// it, and the width of the timestamp column is that of the text it makes.
const SAMPLE_TIME: civil::DateTime = civil::date(2000, 9, 27).at(23, 59, 59, 999_000_000);

impl Default for Config {
    fn default() -> Self {
        let level = |color| Style::new().fg(color);
        Config {
            ui: Ui::default(),
            colors: Colors {
                trace: level(Color::DarkGray),
                debug: level(Color::Cyan),
                info: level(Color::Green),
                warn: level(Color::Yellow),
                error: level(Color::Red),
                fatal: Style::new().fg(Color::White).bg(Color::Red).bold(),
                line_number: level(Color::DarkGray),
            },
            follow: Follow::default(),
            filters: Filters::default(),
            keys: Keymap::default(),
        }
    }
}

impl Default for Ui {
    fn default() -> Self {
        Ui {
            timestamp_format: "%Y-%m-%d %H:%M:%S%.3f".to_string(),
            show_thread: false,
            show_histogram: true,
            show_line_numbers: true,
            mouse: true,
            wrap: false,
        }
    }
}

impl Default for Colors {
    fn default() -> Self {
        Config::default().colors
    }
}

impl Default for Follow {
    fn default() -> Self {
        Follow {
            poll_interval: Duration::from_millis(250),
        }
    }
}

impl Config {
    /// The configuration `text` holds, in TOML. A key it leaves out keeps its
    /// default, so an empty text is the default configuration. Invalid TOML, a key
    /// or section vilog doesn't know, a value of the wrong type, an unknown colour, a
    /// timestamp format strftime can't use, a class rule that doesn't parse and a
    /// `poll_interval_ms` of 0 are errors that say which line they are on and name
    /// the key. So are, in `[keys]`, an action name vilog doesn't know, an empty list
    /// of keys and a key outside the [key syntax](crate::keymap::Key); a key bound
    /// to two actions is an error that names both.
    pub fn parse(text: &str) -> Result<Config, String> {
        let File {
            ui,
            colors,
            follow,
            filters,
            keys,
        } = toml::from_str(text).map_err(|err| err.to_string())?;
        Ok(Config {
            ui,
            colors,
            follow,
            filters,
            keys,
        })
    }
}

impl Colors {
    /// The style of `level` in the level column; the terminal's default style for a
    /// level vilog doesn't know.
    pub fn level(&self, level: Option<Level>) -> Style {
        match level {
            Some(Level::Trace) => self.trace,
            Some(Level::Debug) => self.debug,
            Some(Level::Info) => self.info,
            Some(Level::Warn) => self.warn,
            Some(Level::Error) => self.error,
            Some(Level::Fatal) => self.fatal,
            None => Style::new(),
        }
    }
}

impl Ui {
    /// How many columns the timestamp column needs for `timestamp_format`.
    pub fn timestamp_width(&self) -> usize {
        // Valid by construction: the format was checked while parsing.
        unicode_width::UnicodeWidthStr::width(
            SAMPLE_TIME
                .strftime(&self.timestamp_format)
                .to_string()
                .as_str(),
        )
    }
}

/// Where the configuration file is when `--config` doesn't say:
/// `$XDG_CONFIG_HOME/vilog/config.toml`, or `$HOME/.config/vilog/config.toml` when
/// `XDG_CONFIG_HOME` isn't set or is empty. `None` without either variable. `var`
/// gives the environment variables, `None` for one that isn't set.
pub fn default_path(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    Some(config_home(var)?.join("vilog/config.toml"))
}

/// Where the configuration file was before vilog had its name: [`default_path`]
/// with `jlv` for `vilog`.
pub(crate) fn old_default_path(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    Some(config_home(var)?.join("jlv/config.toml"))
}

fn config_home(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let set = |name| var(name).filter(|value| !value.is_empty());
    match set("XDG_CONFIG_HOME") {
        Some(home) => Some(PathBuf::from(home)),
        None => Some(PathBuf::from(set("HOME")?).join(".config")),
    }
}

fn timestamp_format<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let format = String::deserialize(deserializer)?;
    // Formatting is the only way to learn that a directive is unusable, and an
    // unusable one would panic when a row is drawn.
    let shown = jiff::fmt::strtime::format(&format, SAMPLE_TIME)
        .map_err(|err| de::Error::custom(format!("timestamp_format '{format}': {err}")))?;
    if shown.chars().any(char::is_control) {
        return Err(de::Error::custom(format!(
            "timestamp_format '{format}' makes control characters"
        )));
    }
    Ok(format)
}

fn poll_interval<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Duration, D::Error> {
    match u64::deserialize(deserializer)? {
        0 => Err(de::Error::custom("poll_interval_ms must be at least 1")),
        ms => Ok(Duration::from_millis(ms)),
    }
}

/// The `[keys]` section: action names, each with one key or a list of keys.
fn keys<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Keymap, D::Error> {
    struct KeysVisitor;

    impl<'de> Visitor<'de> for KeysVisitor {
        type Value = Keymap;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a table of action names and keys")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Keymap, A::Error> {
            let mut remaps = Vec::new();
            while let Some(ActionName(action)) = map.next_key()? {
                remaps.push((action, map.next_value_seed(KeyList(action))?));
            }
            Keymap::remapped(&remaps).map_err(de::Error::custom)
        }
    }

    deserializer.deserialize_map(KeysVisitor)
}

/// An action name of the `[keys]` section.
struct ActionName(Action);

impl<'de> Deserialize<'de> for ActionName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        keymap::named(&name)
            .map(ActionName)
            .ok_or_else(|| de::Error::custom(format!("unknown action '{name}'")))
    }
}

/// The keys of one action: a single key or a non-empty list.
struct KeyList(Action);

impl<'de> de::DeserializeSeed<'de> for KeyList {
    type Value = Vec<Key>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Vec<Key>, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for KeyList {
    type Value = Vec<Key>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a key or a list of keys")
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Vec<Key>, E> {
        text.parse().map(|key| vec![key]).map_err(E::custom)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<Key>, A::Error> {
        let mut keys = Vec::new();
        while let Some(KeyText(key)) = seq.next_element()? {
            keys.push(key);
        }
        if keys.is_empty() {
            let name = keymap::name(self.0);
            return Err(de::Error::custom(format!("{name}: no keys")));
        }
        Ok(keys)
    }
}

struct KeyText(Key);

impl<'de> Deserialize<'de> for KeyText {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map(KeyText).map_err(de::Error::custom)
    }
}

fn class_excludes<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Classes, D::Error> {
    let items = Vec::<String>::deserialize(deserializer)?;
    let rules: Vec<String> = items.iter().map(|item| format!("!{item}")).collect();
    Classes::parse(rules.iter().map(String::as_str))
        .map_err(|why| de::Error::custom(format!("default_class_excludes: {why}")))
}

/// A colour name or `#rrggbb`.
struct Hue(Color);

impl<'de> Deserialize<'de> for Hue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        name.parse()
            .map(Hue)
            .map_err(|_| de::Error::custom(format!("unknown colour '{name}'")))
    }
}

/// The inline-table form of a style.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StyleTable {
    fg: Option<Hue>,
    bg: Option<Hue>,
    #[serde(default)]
    bold: bool,
}

fn style<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Style, D::Error> {
    struct StyleVisitor;

    impl<'de> Visitor<'de> for StyleVisitor {
        type Value = Style;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a colour or a table of fg, bg and bold")
        }

        fn visit_str<E: de::Error>(self, name: &str) -> Result<Style, E> {
            Hue::deserialize(de::value::StrDeserializer::new(name))
                .map(|Hue(color)| Style::new().fg(color))
        }

        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Style, A::Error> {
            let StyleTable { fg, bg, bold } =
                StyleTable::deserialize(de::value::MapAccessDeserializer::new(map))?;
            let mut style = Style::new();
            style.fg = fg.map(|hue| hue.0);
            style.bg = bg.map(|hue| hue.0);
            Ok(if bold { style.bold() } else { style })
        }
    }

    deserializer.deserialize_any(StyleVisitor)
}
