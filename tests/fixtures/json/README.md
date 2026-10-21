# JSON-lines fixtures

One file per logging library, each written the way the library writes it by default (PRD 6.2 and 6.4). Every file is one JSON object per line, LF endings, timestamps on 2026-03-14. The table gives the key each column is read from; the exception key is the one a line's exception comes from where the line has one.

| File | timestamp | level | logger | thread | message | exception |
|---|---|---|---|---|---|---|
| `zap.log` | `ts` (epoch seconds, float) | `level` | `logger` | — | `msg` | `stacktrace` (1 of 8 lines) |
| `slog.log` | `time` | `level` | — | — | `msg` | — |
| `pino.log` | `time` (epoch ms) | `level` (numeric, 10..60) | `name` | — | `msg` | `err.stack` (1 of 8 lines, one level deep) |
| `winston.log` | `timestamp` | `level` | — | — | `message` | `stack` (1 of 8 lines) |
| `structlog.log` | `timestamp` | `level` | `logger` | — | `event` | `exception` (1 of 8 lines) |
| `python-json-logger.log` | `asctime` (`2026-03-14 09:12:01,104`) | `levelname` | `name` | — | `message` | `exc_info` (1 of 8 lines) |
| `serilog-clef.log` | `@t` | `@l` (absent means Information) | `SourceContext` | — | `@mt` | `@x` (1 of 8 lines) |
| `ms-json-console.log` | `Timestamp` | `LogLevel` | `Category` | — | `Message` | `Exception` (1 of 8 lines) |
| `logstash-logback.log` | `@timestamp` | `level` | `logger_name` | `thread_name` | `message` | `stack_trace` (1 of 8 lines) |
| `ecs.log` | `@timestamp` | `log.level` (literal dotted key) | `log.logger` (literal dotted key) | `process.thread.name` (literal dotted key) | `message` | `error.stack_trace` (1 of 8 lines, literal dotted key) |
| `docker-json-file.log` | `time` (nanosecond RFC 3339) | — | — | — | `log` (trailing `\n` dropped) | — |

Each file has 8 lines, except `slog.log` with 7. Escapes: every file with an exception has `\n` inside the string, and every line of `docker-json-file.log` ends its `log` value with `\n`. `\"` appears in every file. Non-ASCII text is written as plain UTF-8, not as `\uXXXX` escapes: `Zamówienie 43 przyjęte` in slog, structlog, serilog-clef, ms-json-console, logstash-logback and docker-json-file, `café` in pino.

Level encodings worth asserting: pino numbers 10/20/30/40/50/60; CLEF lines without `@l` are Information; ms-json-console uses `Trace`..`Critical`; python-json-logger uses `WARNING`/`CRITICAL`; structlog uses `warning`/`critical`; winston uses `http` and `verbose`; zap uses `dpanic`.

## Library notes

- **slog** writes an error as an ordinary attribute (`err`, `error`), not under an exception key, so its error line has no exception and `err` is a plain field.
- **Serilog CLEF**: the default compact formatter writes the message template `@mt` with its placeholders unrendered. Only the renderings variant also writes the rendered message `@m`. It leaves out `@l` on Information lines.
- **Microsoft JSON console** writes `Timestamp` only when `TimestampFormat` is configured, as it is here. The message is written twice, as `Message` and as `State.Message`.
- **python-json-logger** writes `asctime` with a comma before the milliseconds: `2026-03-14 09:12:01,104`.
- **pino** writes `name` only for a named logger, as here, and has no thread key (`pid` and `hostname` are not candidates). An error's stack is in `err.stack`, one object deep, next to `err.message`.
- **zap** writes `logger` only for a named logger, as here, and `caller` next to it. An error's text is in `error`, its stack in `stacktrace`.
- **Docker json-file** wraps each line the container printed in `log`, with its trailing newline, next to `stream` and `time`. There is no level, logger or thread: the `INFO` inside the first line's `log` is part of the message.
- **winston** writes a stack under `stack` only with the `errors({ stack: true })` format.
