# vilog

vilog is a terminal log viewer for application logs. It recognises the usual
Logback, Log4j2, Spring Boot and WildFly layouts of Java services, splits each
entry into timestamp, level, class and message columns, folds stack traces, and
follows the file as it grows, through rotation and truncation. You can filter by
level, class, time and text, search, bookmark, copy an entry and export what is
left, all in the terminal and over SSH. It opens multi-gigabyte files without
reading them into memory.

vilog was called jlv before 2.0.

![Following a WildFly server.log during a deploy](docs/demos/s1-follow.gif)

## Install

Prebuilt binaries for Linux x86_64 (static, musl), plus `.deb` and `.rpm`
packages, are on the [releases page](https://github.com/piterx93/vilog/releases).

With Homebrew:

```sh
brew install piterx93/tap/vilog
```

With Rust 1.88 or newer:

```sh
cargo install --locked --git https://github.com/piterx93/vilog
```

Linux x86_64 is the only target CI builds and tests. vilog is developed on macOS
and builds there too.

## Usage

```
Terminal log viewer for application logs

Usage: vilog [OPTIONS] <FILE>

Arguments:
  <FILE>  Log file to open

Options:
  -f, --format <NAME>   Force a format instead of detecting it [possible values: logback,
                        logback-basic, spring3, spring2, wildfly, wildfly-console, raw]
      --no-follow       Open at the top, don't follow
  -l, --level <LEVELS>  Initial level filter, e.g. WARN,ERROR
  -c, --class <RULES>   Initial class filter, e.g. 'com.myco.*,!org.hibernate'
  -s, --since <SPEC>    Initial time filter, e.g. 10m or 12:00-12:30
      --no-mouse        Disable mouse
      --config <PATH>   Config file path
  -h, --help            Print help
  -V, --version         Print version
```

```sh
vilog /opt/wildfly/standalone/log/server.log
vilog --level WARN,ERROR --since 10m server.log
vilog --class 'com.mycompany.*' --no-follow app.log
vilog server.log.2026-10-14.gz
```

vilog opens at the end of the file and follows it like `less +F`: moving up pauses,
and the header counts the entries that arrive meanwhile (`PAUSED +12 new`). `G` or
`End` goes back to the bottom and follows again, `F` toggles following. The dot
after the status is green while the file changed in the last 10 seconds, yellow up
to 5 minutes, grey after that.

Rotation is handled the way `tail -F` handles it: a file that is renamed and
recreated is shown again from the start of the new file, a truncated one shows
`── file truncated ──` and the new content, and a deleted one empties the view
until a file appears at the path again.

Gzip files are recognised by their content, decompressed to a temporary file and
shown read-only. Lines that aren't valid UTF-8 are shown with replacement
characters. vilog needs a terminal of at least 80×24 and turns colours off when
`NO_COLOR` is set.

### Filters

All filters combine. Each one can be set from the command line, from its panel or
with a quick key; `x` clears them all.

- **Level**: `1`–`6` toggle TRACE, DEBUG, INFO, WARN, ERROR and FATAL; `F2` opens
  a panel. `--level` takes a comma-separated list of level names or aliases such
  as `WRN`, `SEVERE` or `CRIT`.
- **Class**: `com.myco.*` keeps a package and everything below it, `OrderService`
  keeps classes containing the text, `!` in front excludes. Includes are or-ed,
  and an exclude always wins. `c` keeps only the selected entry's class, `C` hides
  it; `F6` opens the rules, one per line.
- **Time**: `10m`, `2h`, `1d` are the last minutes, hours or days; `12:00-12:30`,
  `12:00:15-12:00:45` and `12:00-` are ranges on the date of the last entry;
  `2026-10-07 12:00..2026-10-07 13:00` and `2026-10-07 12:00..` carry their own
  date; `12:30` alone is that one minute. "The last N" counts back from the last
  entry in the file, so it works on old logs; in the `F5` panel `Tab` switches to
  the wall clock and `↑ ↓` step through `1m 5m 10m 30m 60m 24h`.
- **Text**: `F4` or `&`, over the message and the stack trace. `Ctrl-r` in the
  prompt toggles regex, `Tab` (or `Ctrl-i`) ignore-case.

Search (`/`, `?`, then `n`, `N`) takes the same pattern options and highlights
matches, and marks folded entries whose stack trace matches. `:` jumps to a line number
(the numbers in the gutter), `t` to a time.

### Copy and export

`y` copies the selected entry with its stack trace. Locally it goes to the system
clipboard. Over SSH (`SSH_CONNECTION` or `SSH_TTY` set) it is sent to your terminal
as an OSC 52 escape sequence, which the terminal has to allow. Inside tmux the
sequence goes through tmux passthrough, which tmux 3.3 and later drop unless
`set -g allow-passthrough on` is in your `tmux.conf`.

`e` or `F8` writes the entries the filters show, with their stack traces and
without ANSI colour codes, to a file. The prompt suggests `<file>.filtered.log` in
the current directory. While the filters are still running over a large file, the
export waits for them; `Esc` drops it.

## Scenarios

**Last 10 minutes, warnings and errors only.** `F5`, `↓` three times to `10m`,
`Enter`, then `1` `2` `3` to hide TRACE, DEBUG and INFO. From the command line:
`vilog -s 10m -l WARN,ERROR server.log`.

![Narrowing to the last 10 minutes and to WARN and up](docs/demos/s2-last-10-minutes.gif)

**Yesterday's incident in a rotated file.** Open the `.gz`, set the time filter to
`14:00-14:15`, `t 14:05` to get there, `]` and `[` to go from exception to
exception, `Enter` for the details.

![Filtering a rotated .gz to 14:00-14:15 and jumping between exceptions](docs/demos/s3-incident-gz.gif)

**Too much noise.** In the class panel, `!org.hibernate` and `!org.jboss` hide
those packages; `com.mycompany.*` then keeps only your own code. Or
`vilog -c '!org.hibernate,!org.jboss' server.log`.

![Hiding org.hibernate and org.jboss, then keeping com.mycompany.*](docs/demos/s4-class-filter.gif)

**Into a ticket.** Go to the error, `Space` to unfold its stack trace, `Enter` for
the details, `y` to copy it, `e` to export the filtered entries.

![Opening an error's details, copying it and exporting the filtered entries](docs/demos/s5-details-export.gif)

The demos are recorded with [vhs](https://github.com/charmbracelet/vhs), which
can't press function keys, so they run with a [config](docs/demos/vilog/config.toml)
that adds `s` for `F5` and `o` for `F6`.

## Formats

The format is detected from the first 200 lines. If no preset matches at least
60% of them, vilog shows the file as raw lines and says so in the header. `P`
cycles through the presets, `--format` picks one.

| `--format` | Example |
|---|---|
| `logback` | `2026-10-07 12:00:00.123 [main] INFO  com.example.App - Started` |
| `logback-basic` | `12:00:00.123 [main] INFO  com.example.App -- Started` |
| `spring3` | `2026-10-07T12:00:00.123+02:00  INFO 12345 --- [app] [           main] o.s.b.SpringApplication                  : Started` |
| `spring2` | `2026-10-07 12:00:00.123  INFO 12345 --- [           main] o.s.b.SpringApplication                  : Started` |
| `wildfly` | `2026-10-07 12:00:00,123 INFO  [org.jboss.as.server] (Controller Boot Thread) WFLYSRV0025: started` |
| `wildfly-console` | `12:00:00,123 INFO  [org.jboss.as] (MSC service thread 1-2) WFLYSRV0049: starting` |
| `raw` | any line, no columns |

`logback` covers Log4j2's classic pattern too. Either `.` or `,` may separate the
milliseconds, and ANSI colour codes are stripped before parsing. Lines that don't
start an entry, such as stack trace lines, belong to the entry above.

Timestamps without an offset are local time. For the time-only formats the date
comes from the file name if it has one (`server.log.2026-10-06`,
`app-2026-10-06.log.gz`), otherwise from the file's modification time, counting
back across midnight.

## Keys

`F1` shows the keys in effect, including remapped ones. The second column is the
action's name in the `[keys]` section of the config.

| Key | Action | Name |
|---|---|---|
| `k` `↑` / `j` `↓` | Move up / down | `up`, `down` |
| `PgUp` / `PgDn` | Page up / down | `page_up`, `page_down` |
| `Ctrl-u` / `Ctrl-d` | Half page up / down | `half_page_up`, `half_page_down` |
| `g` `Home` | Go to top | `top` |
| `G` `End` | Go to bottom and follow | `bottom` |
| `←` / `→` | Scroll left / right | `scroll_left`, `scroll_right` |
| `Tab` | Move focus between table, histogram and details | `cycle_focus` |
| `Enter` | Open or close details; in a panel, apply | `confirm` |
| `Esc` | Drop a waiting export; in a panel, close it | `cancel` |
| `F` | Toggle follow | `toggle_follow` |
| `P` | Cycle format preset | `cycle_preset` |
| `/` / `?` | Search forward / backward | `search_forward`, `search_backward` |
| `n` / `N` | Next / previous match | `next_match`, `previous_match` |
| `:` | Jump to line | `line_prompt` |
| `t` | Jump to time | `time_prompt` |
| `1`–`6` | Toggle TRACE, DEBUG, INFO, WARN, ERROR, FATAL | `toggle_level_trace`, `toggle_level_debug`, `toggle_level_info`, `toggle_level_warn`, `toggle_level_error`, `toggle_level_fatal` |
| `F2` | Level filter | `level_panel` |
| `F4` `&` | Text filter | `text_panel` |
| `F5` | Time filter | `time_panel` |
| `F6` | Class filter | `class_panel` |
| `c` / `C` | Only this class / hide this class | `only_class`, `hide_class` |
| `x` | Clear all filters | `clear_filters` |
| `Space` | Fold or unfold entry | `toggle_fold` |
| `z` / `Z` | Fold all / unfold all | `fold_all`, `unfold_all` |
| `]` / `[` | Next / previous exception | `next_exception`, `previous_exception` |
| `m` | Toggle bookmark | `toggle_bookmark` |
| `'` / `"` | Next / previous bookmark | `next_bookmark`, `previous_bookmark` |
| `y` | Copy entry | `copy` |
| `e` `F8` | Export filtered | `export` |
| `T` | Toggle thread column | `toggle_thread` |
| `#` | Toggle line numbers | `toggle_line_numbers` |
| `H` | Toggle histogram | `toggle_histogram` |
| `w` | Toggle wrap | `toggle_wrap` |
| `F1` | Help | `help` |
| `q` `F10` `Ctrl-c` | Quit | `quit` |

With the histogram focused (`Tab`), `←` `→` pick a bucket and `Enter` sets the time
filter to it. The mouse scrolls, selects a row, folds or unfolds on double-click
and sets the time filter to a histogram bucket on click.

## Configuration

vilog reads `$XDG_CONFIG_HOME/vilog/config.toml`, or `~/.config/vilog/config.toml`
when `XDG_CONFIG_HOME` isn't set, if the file exists. If it doesn't but a
`jlv/config.toml` from before the rename does, vilog reads that one and the header
says where to move it. `--config` names another file.
Every key is optional. This is the full file with the defaults:

```toml
[ui]
# strftime syntax; used from 100 columns on, narrower screens show %H:%M:%S%.3f
timestamp_format = "%Y-%m-%d %H:%M:%S%.3f"
show_thread = false
show_histogram = true
show_line_numbers = true
mouse = true
wrap = false

[colors]
trace = "darkgray"
debug = "cyan"
info = "green"
warn = "yellow"
error = "red"
fatal = { fg = "white", bg = "red", bold = true }
line_number = "darkgray"

[follow]
poll_interval_ms = 250

[filters]
# class rules to hide from the start, without the leading !
default_class_excludes = []   # e.g. ["org.hibernate.SQL", "org.jboss.*"]

[keys]
# e.g. toggle_follow = "f"
```

A colour is a name (`black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`,
`gray`, `darkgray`, `white`, `lightred` and the other `light` ones), `#rrggbb`, or
an index into the 256-colour palette (`"208"`). The table form takes any of `fg`,
`bg` and `bold`.

`--class` replaces `default_class_excludes` rather than adding to them.

An unknown key, a wrong type or a value vilog can't use stops vilog with a message
that names the file and points at the line.

### Remapping keys

Each entry in `[keys]` binds an action, by the names in the table above, to one
key or a list of keys. Those keys replace all of the action's default keys, so
`quit = "Q"` leaves `q`, `F10` and `Ctrl-c` unbound. A key bound to two actions is
an error; to give `F` to another action, rebind `toggle_follow` as well.

```toml
[keys]
toggle_follow = "f"
quit = ["q", "Q", "Ctrl-c"]
level_panel = ["F2", "L"]
```

The key syntax is case-sensitive: a single character (`Q` is Shift-q), `F1`–`F12`,
`Enter`, `Esc`, `Space`, `Tab`, `Up`, `Down`, `Left`, `Right`, `PgUp`, `PgDn`,
`Home`, `End`, and any of these after `Ctrl-`, as `Ctrl-f`. A Ctrl key takes a
lower-case letter. Things to know:

- The help overlay shows the arrow keys as `↑ ↓ ← →`; in the config they are
  `Up`, `Down`, `Left` and `Right`. `"↑"` is accepted, but as the character `↑`,
  which no key sends.
- Remapping `quit` also takes `Ctrl-c` away in the main view. The prompts (search,
  text, class and time filters, jumps, export) always take `Enter`, `Esc` and
  `Ctrl-c`, and the help overlay always closes with `q` and quits with `Ctrl-c`,
  whatever `[keys]` says.
- `Ctrl-Enter`, `Ctrl-Tab` and `Ctrl-Esc` are accepted, but most terminals send
  them as the plain key or not at all, and the same goes for `Ctrl-i`, `Ctrl-m` and
  `Ctrl-[`, which arrive as `Tab`, `Enter` and `Esc`.

## Development

`scripts/check` runs the gates CI runs: formatting, clippy, tests and coverage,
plus mutation testing on uncommitted changes. `docs/demos/render.sh` re-records
the GIFs; it needs vhs, ffmpeg and Python 3.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in vilog by you, as defined in the Apache-2.0 license, shall be dual
licensed as above, without any additional terms or conditions.
