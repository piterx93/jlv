//! The JSON key scanner over a typical pino line (PRD §6.2, §14): every field of the
//! line, at least 1 GB/s. See `docs/benchmarks.md`.

use std::hint::black_box;

use criterion::{Criterion, Throughput};
use vilog::format::json;

/// A pino-http request log line.
const PINO: &[u8] = br#"{"level":30,"time":1791374400123,"pid":48213,"hostname":"orders-api-7d9f8c6b5-x2k4p","reqId":"req-1a2b","req":{"id":"req-1a2b","method":"POST","url":"/orders","query":{},"headers":{"host":"orders.internal:8080","user-agent":"okhttp/4.12.0","content-type":"application/json"},"remoteAddress":"10.4.2.17","remotePort":52100},"res":{"statusCode":201},"responseTime":12,"msg":"request completed"}"#;

fn main() {
    let mut criterion = Criterion::default().configure_from_args();
    let mut group = criterion.benchmark_group("json scanner");
    group.throughput(Throughput::Bytes(PINO.len() as u64));
    group.bench_function("pino line", |b| {
        b.iter(|| json::fields(black_box(PINO)).count());
    });
    group.finish();
    criterion.final_summary();
}
