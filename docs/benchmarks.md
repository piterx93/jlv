# Benchmarks

The targets of PRD §13 are checked by hand, not in CI. The `engine` and `nfr`
benchmarks run on a generated Logback log: a busy service's mix of levels, stack
traces under errors, payload lines of a few kilobytes and, rarely, one over 64 KiB.
By default it is 32 MiB and written to the temp dir for the run.

```sh
cargo bench --bench engine   # criterion: indexing, re-filtering, exception and bookmark jumps
cargo bench --bench nfr      # heap per entry, first screen, idle CPU, follow latency
cargo bench --bench json     # criterion: the JSON key scanner over a pino line, at least 1 GB/s
```

For the PRD's 2 GB, write the log once, outside the repository, and share it:

```sh
export VILOG_BENCH_LOG=/tmp/vilog-bench.log VILOG_BENCH_SIZE=2G
cargo bench --bench engine && cargo bench --bench nfr
rm "$VILOG_BENCH_LOG"
```

`VILOG_BENCH_LOG` is written only if nothing is there yet. The same log, or one in
another preset's layout, comes from
`cargo run --release --example generate_log -- <path> 2G [--format spring3]`.

## What is measured

| Number | How |
|--------|-----|
| Indexing MB/s | `index`: `Indexer` over the whole mapped file until complete, as the viewer runs it. |
| Full re-filter | `re-filter/*`: a new filter generation over the complete index until every entry is evaluated, at the viewer's chunk size. `level` reads the index only; `text` and `regex` read every entry. |
| JSON scanning GB/s | `json`: every key and value span of one fixed 400-byte pino-http line. |
| Exception jump | `exception jump/no hit`: `]` while a level filter hides every entry with an exception, so the whole shown list is looked at. |
| Bookmark jump | `bookmark jump/no hit`: `'` with five bookmarks, all on entries the same level filter hides. |
| Bytes per entry | `nfr`: the 32-byte record, the index `Vec` with its spare capacity, and everything the viewer allocates for a complete index, counted by a global allocator. |
| First screen | `nfr`: from starting the release binary under `script(1)` on a 200x50 pseudo-terminal until the last entry's text is in its output; median of five launches after one unmeasured one. |
| Idle CPU | `nfr`: CPU time of the same binary, from `ps`, over 20 s once indexing is done and nothing writes the file. Linux's `ps` counts whole seconds only. |
| Follow latency | `nfr`: vilog's own event loop and watcher in process, drawing on a `TestBackend`, from appending a line to the frame that shows it; twenty appends at times spread over the loop's 100 ms wait. A pseudo-terminal won't do here: ratatui writes only the cells that change. |

The log is read from the page cache: the run that writes it leaves it there.
