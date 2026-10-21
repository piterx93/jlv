//! A synthetic Java application log for the benchmarks: the levels of a busy
//! service, stack traces under errors and some warnings, now and then a payload line
//! of a few kilobytes and, rarely, one past the viewer's 64 KiB render cap. The same
//! spec always gives the same bytes.
//!
//! Every message starts with `req-<n> `, `n` counting the entries from 0, so that a
//! harness can tell on screen which entry it is looking at.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use jiff::civil::{Date, date};
use vilog::format::{Format, Level};

#[derive(Debug, Clone, Copy)]
pub struct Spec {
    /// Whose layout the entries have. `raw` reads any lines, so it gets Logback's.
    pub preset: &'static Format,
    /// The log ends with the first whole entry that reaches this many bytes.
    pub size: u64,
    pub seed: u64,
}

/// Writes the log `spec` describes to `path`, replacing what is there, a buffer at a
/// time. Returns the number of entries.
pub fn generate(path: &Path, spec: Spec) -> io::Result<u64> {
    let mut out = BufWriter::with_capacity(1 << 20, File::create(path)?);
    let mut log = Log::new(spec);
    let mut entry = Vec::new();
    let mut written = 0;
    while written < spec.size {
        entry.clear();
        log.write_entry(&mut entry);
        out.write_all(&entry)?;
        written += entry.len() as u64;
    }
    out.flush()?;
    Ok(log.seq)
}

/// One INFO entry in `preset`'s layout with `message`, dated after every entry
/// [`generate`] writes, for appending to a generated log.
pub fn line(preset: &'static Format, message: &str) -> Vec<u8> {
    let clock = Clock {
        date: date(2030, 1, 1),
        millis: 0,
    };
    let mut line = Vec::new();
    write_head(&mut line, preset, &clock, Level::Info, "main", PROBE_CLASS);
    line.extend_from_slice(message.as_bytes());
    line.push(b'\n');
    line
}

const PROBE_CLASS: &str = "com.example.bench.Probe";
const PID: u32 = 48213;
const DAY_MILLIS: u32 = 86_400_000;

const THREADS: &[&str] = &[
    "main",
    "http-nio-exec-1",
    "http-nio-exec-2",
    "http-nio-exec-7",
    "scheduling-1",
    "pool-2-thread-3",
    "kafka-consumer",
    "HikariPool-1",
];

// At most 40 characters, so that Spring's `%-40.40logger` keeps them whole.
const CLASSES: &[&str] = &[
    "com.example.shop.ShopApplication",
    "com.example.order.OrderService",
    "com.example.order.OrderRepository",
    "com.example.payment.PaymentClient",
    "com.example.inventory.StockService",
    "o.s.web.servlet.DispatcherServlet",
    "o.h.engine.jdbc.spi.SqlExceptionHelper",
    "com.zaxxer.hikari.pool.HikariPool",
    "o.a.kafka.clients.consumer.KafkaConsumer",
];

const EXCEPTIONS: &[(&str, &str)] = &[
    ("java.lang.IllegalStateException", "Order is already closed"),
    (
        "java.lang.NullPointerException",
        "Cannot invoke \"Customer.getId()\" because \"customer\" is null",
    ),
    (
        "com.example.payment.PaymentException",
        "gateway unavailable after 3 attempts",
    ),
    (
        "org.springframework.dao.DataIntegrityViolationException",
        "could not execute statement",
    ),
    (
        "java.util.concurrent.TimeoutException",
        "timeout after 30000 ms",
    ),
];

const CAUSES: &[(&str, &str)] = &[
    ("java.net.SocketTimeoutException", "Read timed out"),
    (
        "java.sql.SQLIntegrityConstraintViolationException",
        "Duplicate entry for key 'PRIMARY'",
    ),
    ("java.io.IOException", "Broken pipe"),
];

const FRAMES: &[(&str, &str)] = &[
    ("com.example.order.OrderService", "close"),
    ("com.example.order.OrderController", "closeOrder"),
    ("com.example.payment.PaymentClient", "charge"),
    (
        "org.springframework.web.servlet.FrameworkServlet",
        "service",
    ),
    (
        "org.apache.catalina.core.ApplicationFilterChain",
        "doFilter",
    ),
    (
        "java.base/java.util.concurrent.ThreadPoolExecutor",
        "runWorker",
    ),
    ("java.base/java.lang.Thread", "run"),
    ("jdk.proxy2/jdk.proxy2.$Proxy142", "invoke"),
];

struct Log {
    preset: &'static Format,
    rng: Rng,
    clock: Clock,
    seq: u64,
}

impl Log {
    fn new(spec: Spec) -> Log {
        Log {
            preset: spec.preset,
            rng: Rng(spec.seed),
            clock: Clock {
                date: date(2026, 10, 7),
                millis: 8 * 3_600_000,
            },
            seq: 0,
        }
    }

    fn write_entry(&mut self, out: &mut Vec<u8>) {
        self.clock.advance(self.rng.below(20) as u32);
        let level = self.level();
        let thread = self.rng.pick(THREADS);
        let class = self.rng.pick(CLASSES);
        write_head(out, self.preset, &self.clock, level, thread, class);
        // Writing to a Vec can't fail, here and below.
        let _ = write!(out, "req-{} ", self.seq);
        self.write_message(out, level);
        out.push(b'\n');
        let traced = match level {
            Level::Error => self.rng.per_mille(700),
            Level::Warn => self.rng.per_mille(100),
            _ => false,
        };
        if traced {
            self.write_trace(out);
        }
        self.seq += 1;
    }

    fn level(&mut self) -> Level {
        match self.rng.below(1000) {
            0..30 => Level::Trace,
            30..280 => Level::Debug,
            280..880 => Level::Info,
            880..960 => Level::Warn,
            _ => Level::Error,
        }
    }

    fn write_message(&mut self, out: &mut Vec<u8>, level: Level) {
        let (n, ms) = (self.rng.below(100_000), self.rng.below(5000));
        let _ = match (level, self.rng.below(3)) {
            (Level::Trace, 0) => write!(out, "Tick"),
            (Level::Trace, _) => write!(out, "Entering OrderService.find(id={n})"),
            (Level::Debug, 0) => write!(out, "Checked {} lines: [sku-{n}, sku-{ms}]", ms % 9),
            (Level::Debug, 1) => write!(out, "select o from Order o where o.id = {n} took {ms} ms"),
            (Level::Debug, _) => write!(out, "Flushed 500 rows [offset={n}]"),
            (Level::Info, 0) => write!(out, "Order {n} created for customer {ms}"),
            (Level::Info, 1) => write!(out, "GET /api/orders/{n} 200 in {ms} ms"),
            (Level::Info, _) => write!(
                out,
                "User {ms} logged in from 10.0.{}.{}",
                n % 256,
                ms % 256
            ),
            (Level::Warn, 0) => write!(out, "Retry {}/3 - gateway answered 503", ms % 3 + 1),
            (Level::Warn, 1) => write!(out, "Slow query: {ms} ms"),
            (Level::Warn, _) => write!(out, "Connection pool at {}% capacity", 80 + ms % 20),
            (Level::Error, 0) => write!(out, "Payment for order {n} failed"),
            _ => write!(out, "Request processing failed: timeout after {ms} ms"),
        };
        let items = if self.rng.below(200_000) == 0 {
            4000 + self.rng.below(4000)
        } else if self.rng.per_mille(5) {
            40 + self.rng.below(560)
        } else {
            return;
        };
        self.write_payload(out, items);
    }

    /// About 28 bytes per item.
    fn write_payload(&mut self, out: &mut Vec<u8>, items: u64) {
        out.extend_from_slice(b" payload={\"items\":[");
        for i in 0..items {
            if i > 0 {
                out.push(b',');
            }
            let (sku, qty) = (self.rng.below(10_000), self.rng.below(9) + 1);
            let _ = write!(out, "{{\"sku\":\"sku-{sku}\",\"qty\":{qty}}}");
        }
        out.extend_from_slice(b"]}");
    }

    fn write_trace(&mut self, out: &mut Vec<u8>) {
        let (exception, text) = self.rng.pick(EXCEPTIONS);
        let _ = writeln!(out, "{exception}: {text}");
        let frames = 4 + self.rng.below(20);
        self.write_frames(out, frames);
        if self.rng.per_mille(400) {
            let (cause, text) = self.rng.pick(CAUSES);
            let _ = writeln!(out, "Caused by: {cause}: {text}");
            let frames = 2 + self.rng.below(6);
            self.write_frames(out, frames);
            let _ = writeln!(out, "\t... {} more", 3 + self.rng.below(10));
        }
    }

    fn write_frames(&mut self, out: &mut Vec<u8>, count: u64) {
        for _ in 0..count {
            let (class, method) = self.rng.pick(FRAMES);
            let file = class.rsplit(['.', '/']).next().unwrap_or(class);
            let line = 20 + self.rng.below(1500);
            let _ = writeln!(out, "\tat {class}.{method}({file}.java:{line})");
        }
    }
}

/// An entry's first line up to where the message starts.
fn write_head(
    out: &mut Vec<u8>,
    preset: &'static Format,
    clock: &Clock,
    level: Level,
    thread: &str,
    class: &str,
) {
    let level = level.name();
    let _ = match preset.id {
        "logback" | "raw" => {
            clock.write_date_time(out, ' ', '.');
            write!(out, " [{thread}] {level:<5} {class} - ")
        }
        "logback-basic" => {
            clock.write_time(out, '.');
            write!(out, " [{thread}] {level:<5} {class} -- ")
        }
        "spring3" => {
            clock.write_date_time(out, 'T', '.');
            write!(
                out,
                "+02:00 {level:>5} {PID} --- [shop] [{thread:>15}] {class:<40} : "
            )
        }
        "spring2" => {
            clock.write_date_time(out, ' ', '.');
            write!(out, " {level:>5} {PID} --- [{thread:>15}] {class:<40} : ")
        }
        "wildfly" => {
            clock.write_date_time(out, ' ', ',');
            write!(out, " {level:<5} [{class}] ({thread}) ")
        }
        "wildfly-console" => {
            clock.write_time(out, ',');
            write!(out, " {level:<5} [{class}] ({thread}) ")
        }
        other => panic!("no layout written for {other}"),
    };
}

/// The time of the entry being written, kept as numbers so that writing it costs a
/// few digits rather than a time zone lookup.
struct Clock {
    date: Date,
    millis: u32,
}

impl Clock {
    fn advance(&mut self, millis: u32) {
        self.millis += millis;
        if self.millis >= DAY_MILLIS {
            self.millis -= DAY_MILLIS;
            // Fails only past the year 9999, a few million years of entries away.
            self.date = self.date.tomorrow().expect("the log ends before 9999");
        }
    }

    fn write_date_time(&self, out: &mut Vec<u8>, separator: char, fraction: char) {
        let date = self.date;
        let _ = write!(
            out,
            "{:04}-{:02}-{:02}{separator}",
            date.year(),
            date.month(),
            date.day()
        );
        self.write_time(out, fraction);
    }

    fn write_time(&self, out: &mut Vec<u8>, fraction: char) {
        let seconds = self.millis / 1000;
        let _ = write!(
            out,
            "{:02}:{:02}:{:02}{fraction}{:03}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60,
            self.millis % 1000
        );
    }
}

/// SplitMix64: small, fast and the same everywhere, which is all a generator of test
/// data needs.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number in `0..bound`, close enough to uniform for small bounds.
    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    fn per_mille(&mut self, chance: u64) -> bool {
        self.below(1000) < chance
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len() as u64) as usize]
    }
}

/// A size like `2G`, `64M`, `512K` (powers of 1024) or a plain number of bytes.
pub fn parse_size(text: &str) -> Result<u64, String> {
    let (digits, unit) = match text.char_indices().last() {
        Some((at, 'K' | 'k')) => (&text[..at], 1 << 10),
        Some((at, 'M' | 'm')) => (&text[..at], 1 << 20),
        Some((at, 'G' | 'g')) => (&text[..at], 1 << 30),
        _ => (text, 1),
    };
    digits
        .parse::<u64>()
        .ok()
        .and_then(|count| count.checked_mul(unit))
        .ok_or_else(|| format!("invalid size {text:?}, expected e.g. 2G, 64M or 4096"))
}
