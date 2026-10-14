# PRD: `jlv` — interactive TUI log viewer for Java logs

> Working name: **jlv** (Java Log Viewer) — to be finalized.
> Status: Draft v0.2 · 2026-10-08

---

## 1. Summary

`jlv` is a fast, interactive terminal log viewer — in the spirit of `lnav` / LogExpert, with the feel of `htop` / `btop` — focused on Java application logs (SLF4J/Logback, Log4j2, Spring Boot, WildFly/JBoss). It parses each log entry into structured columns, colors severities, follows files live (`tail -F` semantics), and lets the user scroll, search, and filter by level, class, time, and text without leaving the terminal.

```
| TIMESTAMP | SEVERITY | CLASS | MESSAGE |
```

## 2. Problem

- `tail -f | grep` loses context, can't scroll back comfortably, and doesn't understand multi-line stack traces.
- `less` doesn't understand log structure (levels, classes, time).
- `lnav` is powerful but generic; Java-specific needs (stack-trace folding, logger-name abbreviation, package-prefix filters, WildFly/Spring formats out of the box) are second-class.
- LogExpert is Windows/GUI-only and unusable over SSH.

## 3. Goals

1. Open multi-GB Java log files instantly and scroll smoothly.
2. Recognize common Java log formats automatically — zero configuration for the 90% case.
3. Live follow that survives log rotation.
4. Fast, composable filtering by **level**, **class/package**, **time window**, and **text/regex**.
5. Work identically over SSH and locally, shipped as a single static Linux binary.

### Non-goals (v1)

- Multiple files in one view (planned v2).
- stdin / pipe input (planned v2).
- JSON/structured logs (planned v2).
- Custom formats via Logback pattern or regex (planned v2).
- Editing logs, remote log fetching (SSH/K8s APIs), alerting, persistence/database.

## 4. Users & key scenarios

Target: Java developers, DevOps/SRE, support engineers. Open source project.

| # | Scenario |
|---|----------|
| S1 | SSH into a server, `jlv /opt/wildfly/standalone/log/server.log`, watch live traffic during a deploy. |
| S2 | "What happened in the last 10 minutes?" — one keypress narrows to the last 10 min, toggle only WARN+ERROR. |
| S3 | Incident at 14:05 yesterday: open the rotated `.gz`, filter `14:00-14:15`, jump between exceptions. |
| S4 | Too much noise: hide `org.hibernate`, `org.jboss`, show only `com.mycompany.*`. |
| S5 | Found the relevant error: open details, expand the stack trace, copy it to clipboard, export the filtered slice to a file for a ticket. |

## 5. Platform & distribution

- **Language/stack:** Rust + `ratatui` (`crossterm` backend).
- **Platforms:** Linux x86_64 (static musl build). That's the only target that is built and tested. Other targets (aarch64, macOS) may come later if someone needs them.
- **Local development:** `scripts/dev` runs any command in a Debian-based Rust container (`docker/dev.Dockerfile`), so Linux-specific behaviour (inotify, musl) gets tested on a macOS machine too. `JLV_PLATFORM=linux/amd64` switches to the exact CI architecture.
- **Terminals:** must work with 256 colors and a minimum 80×24 terminal; use truecolor when available; respect `NO_COLOR`.
- **Distribution:** GitHub Releases (prebuilt binaries), `.deb` / `.rpm` packages; `cargo install` and a Homebrew tap once the repo is public.
- **Visibility:** the repo stays private until 1.0 and is open-sourced after that.
- **License:** MIT OR Apache-2.0.

### 5.1 CI/CD (GitHub Actions) — set up first, in M0

**Branching model:** `feature/*` → PR into `develop` → PR into `main`.

| Trigger | Tests | Packages | Version | Published to |
|---------|-------|----------|---------|--------------|
| Push to any other branch, PRs into `develop`/`main` | yes | no | — | — |
| Push to `develop` | yes | yes | `X.Y.Z-dev.<run>` (e.g. `0.3.0-dev.42`) | Workflow artifacts + rolling GitHub **pre-release** `dev` (replaced on every push) |
| Push to `main` | yes | yes | `X.Y.Z` from `Cargo.toml` | GitHub **Release** `vX.Y.Z` |

`jlv --version` also prints the short commit hash for CI builds, e.g. `jlv 0.3.0-dev.42 (a1b2c3d)`.

**Test job** (runs on every trigger, `ubuntu-latest`):
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --all`
- `cargo llvm-cov --fail-under-lines 80` (line coverage of the library crate)
- A final aggregate job, **`ci-required`**, fails if any check job failed. This gives branch protection one stable check name to require, even if the check jobs are later split or extended.

Mutation testing doesn't run in CI: on a hosted runner the full suite takes hours of paid minutes. It runs locally in the dev container before a change is pushed (§14.1).

**Package job** (only `develop` and `main`; `needs: ci-required`, so it never runs on red tests):
- Target: `x86_64-unknown-linux-musl`. The built binary is smoke-tested with `--version`.
- Artifacts: `jlv-<version>-<target>.tar.gz`, `.deb` (`cargo-deb`), `.rpm` (`cargo-generate-rpm`), `SHA256SUMS`.
- The version is injected at build time via `JLV_VERSION` / `JLV_COMMIT`; CI never edits `Cargo.toml`. On `develop`, `Cargo.toml` holds the *next* planned version, so it must be bumped right after each release.
- deb/rpm get `X.Y.Z~dev.<run>`, so that package managers sort dev builds before the release.
- On `main`, the job fails if the tag `vX.Y.Z` already exists. This forces a version bump before each release.

**Merge blocking** (GitHub repository ruleset on `develop` and `main`). **Disabled until the repo goes public at 1.0.** The `gh api` call in the script is commented out.
- Changes must go through a pull request; direct pushes are not allowed.
- The required status check **`ci-required`** must pass, and the branch must be up to date before merging.
- Force-pushes and branch deletion are blocked.
- The ruleset lives in `scripts/branch-ruleset.json` and is applied by `scripts/setup-branch-protection.sh`, so it can be reproduced.

## 6. Log formats

### 6.1 Built-in presets (v1)

| Preset | Example line |
|--------|--------------|
| **Logback / Log4j2 classic** | `2026-10-07 12:00:00.123 [http-nio-8080-exec-3] INFO  com.example.OrderService - Order 42 created` |
| **Logback default (BasicConfigurator)** | `12:00:00.123 [main] INFO  com.example.App -- Started` |
| **Spring Boot 3.x** | `2026-10-07T12:00:00.123+02:00  INFO 12345 --- [myapp] [           main] o.s.b.SpringApplication                  : Started App in 2.1s` |
| **Spring Boot 2.x** | `2026-10-07 12:00:00.123  INFO 12345 --- [           main] o.s.b.SpringApplication                  : Started App` |
| **WildFly server.log** | `2026-10-07 12:00:00,123 INFO  [org.jboss.as.server] (Controller Boot Thread) WFLYSRV0025: WildFly Full 33.0.0.Final started` |
| **WildFly console** | `12:00:00,123 INFO  [org.jboss.as] (MSC service thread 1-2) WFLYSRV0049: ...` |
| **Raw (fallback)** | Any line; no columns, plain viewing/search only. |

Each preset defines: an entry-start regex with named groups (`timestamp`, `level`, `thread`, `class`, `message`), a timestamp format, and a flag for whether the date is present.

Variants to tolerate: `.` or `,` as the millisecond separator; `-` or `--` before the message; optional Spring app-name group; padded levels (`INFO `); ANSI color escapes (stripped before parsing — WildFly console logs often contain them).

> **Open item:** validate presets against real sample logs from the author (see §15).

### 6.2 Auto-detection

- Sample the first ~200 non-empty lines (for `.gz`: the first decompressed lines).
- Score each preset by the ratio of lines matching its entry-start regex. Continuation lines (stack traces) are ignored in the denominator if they look like continuations (see 6.3).
- Pick the highest score ≥ 60%; otherwise fall back to **Raw** and show a status-bar notice.
- User override: `--format <preset>` CLI flag and `P` to cycle presets at runtime.

### 6.3 Multi-line entries (stack traces)

- Any line not matching the entry-start regex belongs to the previous entry.
- An entry contains an **exception** if a continuation line matches `^\s+at `, `^Caused by: `, `^\s+\.\.\. \d+ (more|common frames omitted)`, or a `^[\w.$]+(Exception|Error)(: |$)` header. The first line of an entry with a recognised start is never checked, so a message like `Error: …` doesn't count; a headless entry (a file that starts mid-trace) has all of its lines checked.
- In the list view, multi-line entries are **folded** by default and show a marker (e.g. `[+37]`). The user can expand or collapse one entry, or all entries.
- Keys exist to jump to the next or previous entry with an exception.

### 6.4 Severity levels & colors

| Level | Color (default) |
|-------|-----------------|
| TRACE | gray / dim |
| DEBUG | cyan |
| INFO | green |
| WARN (alias WARNING) | yellow |
| ERROR | red |
| FATAL | bold white on red |
| unknown | default fg |

Colors are configurable (§11).

### 6.5 Timestamps

- If the format has a full date: parse it; use the explicit offset if present, otherwise local time.
- If the format has **time only** (WildFly console, Logback default): take the date of the last entry from the file name if it contains one (`server.log.2026-10-06`, `app-2026-10-06.log.gz`), otherwise from the file's mtime, taken with its time of day so that a file flushed just after midnight isn't dated a day late. Then walk backwards. When the earlier entry's time of day is more than 2 h *after* the later one, midnight was crossed, so decrement the day; smaller steps back are out-of-order logging or a DST fall-back. When the earlier entry's time of day is more than 23 h 59 min *before* the later one, a late thread wrote the later entry just before midnight, so the earlier entry belongs to the next day.
- Entries without a parseable timestamp inherit the timestamp of the previous entry (used for time filters and the histogram).

## 7. UI

### 7.1 Layout

```
┌ jlv ─ server.log ─ 1.8 GB ─ WildFly ────────────── FOLLOW ● ── E:37 W:212 ──┐
│ Filters: level[W E F]  class[com.myco.* !org.hibernate]  time[last 10m]      │
├──────────────────────────────────────────────────────────────────────────────┤
│ ▁▁▂▁▁▃▁▁▁▁▂▁▁▁▁▁▁▇▅▁▁▁▁▁▂▁▁▁▁▁▁▁▁▁▁▁▃▁▁▁ (errors+warns per minute)            │
├─────────────────────┬───────┬──────────────────────┬─────────────────────────┤
│ TIMESTAMP           │ LEVEL │ CLASS                │ MESSAGE                 │
│ 2026-10-07 12:00:01 │ INFO  │ c.m.order.OrderSvc   │ Order 42 created        │
│ 2026-10-07 12:00:02 │ WARN  │ c.m.pay.PayClient    │ Retry 1/3 ...           │
│▶2026-10-07 12:00:03 │ ERROR │ c.m.pay.PayClient    │ Payment failed [+37]    │
├─────────────────────┴───────┴──────────────────────┴─────────────────────────┤
│ Details: com.mycompany.pay.PayClient (default task-12)                       │
│ java.net.SocketTimeoutException: Read timed out                              │
│     at java.base/java.net.SocketInputStream.read(...)                        │
├──────────────────────────────────────────────────────────────────────────────┤
│ F1Help F2Level F3Search F4Filter F5Time F6Class F7Bkmk F8Export F10Quit      │
└──────────────────────────────────────────────────────────────────────────────┘
```

- **Header:** file name, size, detected format, follow status, number of new lines since pause, counters per level. The counters respect the class, time and text filters but ignore the level filter, so hidden levels don't drop to zero.
- **Filter bar:** active filters; hidden when there are none.
- **Histogram (toggleable):** WARN/ERROR counts per time bucket. The bucket size adapts to the visible range. Selecting a bucket (mouse click or keys) sets the time filter to that bucket.
- **Table:** Timestamp | Level | Class | Message, plus optional **Thread** (hidden by default, toggle key). In v2 a **Source** column is added for multi-file views.
- **Details panel (toggleable, Enter):** full class name, thread, full message, fully expanded stack trace, scrollable.
- **Footer:** htop-style function-key hints.

### 7.2 Columns

- Timestamp format in the table is configurable (default `yyyy-MM-dd HH:mm:ss.SSS`, or `HH:mm:ss.SSS` when narrow).
- Class names are abbreviated Logback-style to fit the column (`com.mycompany.pay.PayClient` → `c.m.pay.PayClient` → `c.m.p.PayClient`). The full name is shown in Details.
- Long messages are truncated by default with horizontal scroll (`←/→`); a toggle switches to wrapping.
- A gutter left of the table shows file line numbers in a dim colour (first line of each entry, every line of an unfolded one). Shown by default, `#` hides it.

### 7.3 Mouse

Wheel scrolls; click selects a row; double-click toggles fold; click on a histogram bucket sets the time filter. Mouse support can be disabled in config, since some SSH and tmux setups don't handle it well.

## 8. Follow mode

- Starts in follow mode at end of file (override `--no-follow`, which opens at the top).
- Behaves like `less +F`. Any move away from the bottom **pauses** follow. The header shows `FOLLOW ●` while following and `PAUSED +N new` once entries arrive during a pause (N counts entries, not lines). `F` toggles follow; `End` and `G` jump to the bottom and resume.
- File watching uses `notify` (inotify/kqueue), with a polling fallback every 250 ms for NFS and similar filesystems.
- **Rotation handling** (`tail -F` semantics):
  - inode change (rename+create, or delete and recreate): finish reading the old file, then reopen the path and show the new file from its start.
  - size shrink (copytruncate): drop the rows of the old content, whose bytes are gone, and show `── file truncated ──` as the first row, followed by the new content.
  - if the file disappears, wait and keep retrying; show `WAITING FOR FILE` in the header.
- Active filters apply live to new entries.
- `.gz` files (detected by the gzip magic bytes, not the name) are read-only: never followed, `READ-ONLY` in the header. A damaged file shows what could be decompressed, with `truncated .gz` in the header. Progress is printed on stderr while decompressing, before the TUI starts.

## 9. Navigation, search & filter

### 9.1 Navigation

`j/k`, `↑/↓` move by line; `PgUp/PgDn` move by page, `Ctrl-u/d` by half a page; `g/G` and `Home/End` go to the top or bottom; `:<n>` goes to the entry containing file line n (the numbers in the gutter); `t` jumps to a time (`t 12:30`).

### 9.2 Search (highlight & jump)

- `/` searches forward and `?` searches backward; `n/N` go to the next or previous match.
- Modes: plain (default), regex, case-insensitive. Toggled in the prompt with `Ctrl-r` and `Ctrl-i` (terminals send `Ctrl-i` as `Tab`, so both work).
- All visible matches are highlighted. The status bar shows `match 3/57`.
- Search also covers folded stack-trace lines; a match inside a folded entry marks the entry.

### 9.3 Filters (hide non-matching)

All active filters combine with **AND** and apply live, including in follow mode. `x` clears all filters; `Esc` closes a filter panel without applying pending edits.

| Filter | Behavior | Keys |
|--------|----------|------|
| **Level** | Independent toggles per level | `1`–`6` = TRACE…FATAL, `F2` opens panel |
| **Class** | List of rules: `com.myco.*` (prefix include), `OrderService` (substring include), `!org.hibernate` (exclude). Includes are OR'd together; excludes always win. | `F6` panel; `c` = "only this class" from selected row; `C` = "hide this class" |
| **Time** | Presets: last 1/5/10/30/60 min, 24 h. Range input: `12:00-12:30`, `12:00:15-12:00:45`, `12:00-` (open-ended), full `2026-10-07 12:00..2026-10-07 13:00`. A range without a date uses the date of the last entry. | `F5` panel; quick keys for presets |
| **Text** | Plain/regex/ignore-case over message + continuation lines | `F4` / `&` |

**Time base:** "last N minutes" is relative to the **last entry in the file** by default, so it works for old logs. A toggle (`F5` panel) switches to the **wall clock**. The active base is shown in the filter bar (`last 10m (file)` / `last 10m (now)`).

### 9.4 Bookmarks

`m` toggles a bookmark on the selected entry; `'` / `"` jump to the next or previous bookmark; bookmarks are shown with a gutter marker. Bookmarks last for the session only in v1.

### 9.5 Export & clipboard

- `F8` / `e` exports the currently filtered entries (original lines, including stack traces, with ANSI escapes stripped) to a file. The prompt suggests `<name>.filtered.log` in the current directory.
- `y` copies the selected entry (with stack trace, ANSI escapes stripped) to the clipboard via **OSC 52**, which works over SSH. Locally there is a native clipboard fallback.

## 10. CLI

```
jlv [OPTIONS] <FILE>

  -f, --format <PRESET>     Force format (logback, logback-basic, spring3, spring2,
                            wildfly, wildfly-console, raw)
      --no-follow           Open at top, don't follow
  -l, --level <LEVELS>      Initial level filter, e.g. WARN,ERROR
  -c, --class <RULES>       Initial class filter, e.g. 'com.myco.*,!org.hibernate'
  -s, --since <SPEC>        Initial time filter, e.g. 10m or 12:00-12:30
      --config <PATH>       Config file path
      --no-mouse            Disable mouse
```

## 11. Configuration

`~/.config/jlv/config.toml` (XDG):

```toml
[ui]
timestamp_format = "%Y-%m-%d %H:%M:%S%.3f"
show_thread = false
show_histogram = true
show_line_numbers = true
mouse = true
wrap = false

[colors]
trace = "darkgray"
debug = "cyan"
info  = "green"
warn  = "yellow"
error = "red"
fatal = { fg = "white", bg = "red", bold = true }
line_number = "darkgray"

[follow]
poll_interval_ms = 250

[filters]
default_class_excludes = []   # e.g. ["org.hibernate.SQL"]

[keys]
# override any binding, e.g.
# toggle_follow = "F"
```

## 12. Key bindings (defaults)

| Key | Action |
|-----|--------|
| `F1` | Help overlay (`?` is backward search) |
| `j k ↑ ↓` | Move |
| `PgUp PgDn Ctrl-u Ctrl-d` | Page |
| `g G Home End` | Top / bottom |
| `F` | Toggle follow |
| `P` | Cycle format preset |
| `/` `?` `n` `N` | Search |
| `F2`, `1`–`6` | Level filter |
| `F4` / `&` | Text filter |
| `F5` | Time filter |
| `F6`, `c`, `C` | Class filter / only this / hide this |
| `x` | Clear all filters |
| `Enter` | Details panel |
| `Space` / `z` / `Z` | Fold toggle / fold all / unfold all |
| `]` / `[` | Next / previous exception |
| `m` `'` `"` | Bookmarks |
| `y` | Copy entry |
| `F8` / `e` | Export filtered |
| `T` | Toggle thread column |
| `#` | Toggle line numbers |
| `H` | Toggle histogram |
| `w` | Toggle wrap |
| `← →` | Horizontal scroll |
| `q` / `F10` | Quit |

## 13. Non-functional requirements

| Area | Requirement |
|------|-------------|
| Startup | First screen (tail of file) visible in < 300 ms for a 2 GB file; indexing continues in the background with a progress indicator. |
| Indexing | ≥ 500 MB/s on SSD for the entry index. |
| Memory | Index ≤ ~32 bytes per entry; the file's content is never loaded into memory in full (mmap + on-demand parsing of visible rows). |
| Filtering | Full re-filter of 2 GB in < 3 s, running in the background and cancellable; results stream in progressively. |
| Rendering | Scrolling stays smooth (no frame > 16 ms) regardless of file size. |
| Follow latency | New line visible < 250 ms after write. |
| Idle | ~0% CPU when the file isn't changing. |
| Robustness | Never crashes on invalid UTF-8 (lossy decode), binary garbage, or extremely long lines (cap rendering, e.g. at 64 KB per line). |

## 14. Architecture (initial)

```
           ┌─────────────┐   events   ┌──────────────┐
 file ───▶ │ Source      │──────────▶ │ Indexer      │  entry index (offset, len,
 (.log/.gz)│ mmap/notify │            │ (bg thread)  │  ts, level, class span, flags)
           └─────────────┘            └──────┬───────┘
                                             │
                                      ┌──────▼───────┐
                                      │ Filter engine│  Vec<u32> of matching entries,
                                      │ (bg, cancel) │  incremental on append
                                      └──────┬───────┘
                                             │
                                      ┌──────▼───────┐
                                      │ App / UI     │  ratatui; parses only visible rows
                                      └──────────────┘
```

- **Source:** `memmap2` for plain files (remap on growth); `.gz` is decompressed to a temp file once (via `flate2`) and then treated as a plain file.
- **Index:** a compact struct per entry, built in chunks; it grows on append, and rotation triggers a rebuild.
- **Filter engine:** level/time/class checks use index fields only, so they are fast. Text and regex checks read entry bytes. Each filter generation can be cancelled.
- **Crates:** `ratatui`, `crossterm`, `notify`, `memmap2`, `regex`, `jiff`, `flate2`, `serde` + `toml`, `clap`, `arboard` (local clipboard), `directories`.
- **Layout:** one crate with a library and a thin binary. `src/main.rs` only parses arguments and runs the app; everything else lives in library modules (`source`, `format`, `index`, `filter`, `follow`, `app`, `ui`, `config`). `format`, `index` and `filter` must not depend on `ui` or `ratatui`.

### 14.1 Testing & quality

- **Test-first.** Every feature starts with failing tests that describe the behaviour, including edge cases. Implementation changes only non-test code; if a test turns out to be wrong, it is fixed as a separate step.
- **Unit tests** live in a `tests.rs` file next to the module (`#[cfg(test)] mod tests;`), not inline, so test and production code change in separate files.
- **Integration tests** in `tests/`: fixture logs per preset (synthetic, including stack traces, midnight rollover and ANSI codes) and rotation scenarios (rename, copytruncate, delete+recreate).
- **UI** is tested by rendering into ratatui's `TestBackend` and comparing against inline `insta` snapshots written by hand as the expected screen. App state and key handling are tested without rendering.
- **Property tests** (`proptest`) for parsers, class abbreviation and the filter engine, e.g. "never panics on arbitrary bytes", "engine result equals a naive filter".
- **Mutation testing** (`cargo mutants`): no surviving mutants in changed code before a change is pushed; equivalent mutants are excluded explicitly with a reason. The full run is started by hand, locally.
- **Coverage:** at least 80% line coverage, enforced in CI.
- **Benchmarks:** `criterion` on a generated 2 GB synthetic log, run manually to verify §13; not part of CI.

## 15. Milestones

| Milestone | Scope |
|-----------|-------|
| **M0** Skeleton | **First:** GHA pipeline per §5.1 (tests on every branch, `-dev` packages on `develop`, releases on `main`; merge blocking prepared but disabled). Then: quality gates (§14.1), CLI parsing, raw viewer with scroll |
| **M1** Parsing | Presets + auto-detection, columns, level colors, multi-line folding, class abbreviation, background index |
| **M2** Follow | notify + polling, rotation & truncation handling, `less +F` pause/resume, `.gz` read |
| **M3** Search & filter | Search (plain/regex/icase), level/class/time/text filters, live filtering, filter bar, line numbers |
| **M4** UX | Details panel, exception jumping, bookmarks, export, clipboard (OSC 52), histogram, mouse, help overlay |
| **M5** Release 1.0 | Config file, key remapping, docs/README with GIFs, LICENSE files, enable branch protection, make the repo public, Homebrew tap |

### v2 backlog

- Multiple files merged chronologically, with a Source column
- stdin / pipe input (`kubectl logs -f | jlv -`)
- JSON logs (logstash-logback-encoder, ECS)
- Custom formats: Logback pattern → parser, named-group regex in config
- Saved filter presets
- JUL levels (SEVERE/FINE/…), MDC columns (traceId, requestId)
- Persistent bookmarks

## 16. Open questions

1. **Final name:** `jlv` is a placeholder.

### Resolved (v0.2)

- **Sample logs:** presets are validated against synthetic fixtures built from the examples in §6.1 and the upstream docs of each framework.
- **Date for time-only formats:** a date in the file name wins over mtime (§6.5).
- **Level counters:** current filter scope without the level filter (§7.1).
- **Source column:** not in v1; it arrives with multi-file views in v2.
- **`?` key:** backward search; the help overlay is `F1` only.
