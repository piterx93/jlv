//! The table of every preset fixture at 80×24 and 120×30, opened at the top.

use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use insta::assert_snapshot;
use jiff::tz::TimeZone;
use jlv::app::{Action, App};
use jlv::format::Preset;
use jlv::source;
use jlv::ui::{Theme, render};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

/// The `width`×`height` screen of the fixture `name` read in `preset`, opened at the
/// top with the whole file indexed, without line numbers. Timestamps are shown in
/// Warsaw time, the offset the Spring Boot 3 fixture writes; time-only logs are
/// dated from a modification time on the morning of 2026-10-08.
fn screen(preset: Preset, name: &str, width: u16, height: u16) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let mut app = App::new(
        &path,
        source::open(&path).unwrap(),
        Some(preset),
        TimeZone::get("Europe/Warsaw").unwrap(),
        "2026-10-08T06:00:00Z".parse().unwrap(),
        false,
    );
    app.apply(Action::ToggleLineNumbers);
    let deadline = Instant::now() + Duration::from_secs(1);
    app.poll_index();
    while !app.progress().is_complete() {
        assert!(Instant::now() < deadline, "{:?}", app.progress());
        thread::sleep(Duration::from_millis(1));
        app.poll_index();
    }
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| render(frame, &mut app, Theme::Styled))
        .unwrap();
    terminal.backend().to_string()
}

#[test]
fn logback_at_80_by_24() {
    let screen = screen(Preset::Logback, "logback.log", 80, 24);
    assert_snapshot!(screen, @r#"
    "jlv ─ logback.log ─ 4.7 KB ─ Logback                                            "
    "▶09:14:02.118 INFO  c.e.shop.ShopApplication Starting ShopApplication v1.4.2 usi"
    " 09:14:02.121 DEBUG c.e.shop.ShopApplication Running with Spring Boot v3.3.4, Sp"
    " 09:14:03.502 INFO  o.s.b.w.e.t.TomcatWebSe… Tomcat initialized with port 8080 ("
    " 09:14:04.877 INFO  c.z.h.HikariDataSource   HikariPool-1 - Start completed.    "
    " 09:14:06.010 INFO  c.e.shop.ShopApplication Started ShopApplication in 4.213 se"
    " 09:15:11.342 INFO  c.e.order.OrderService   Order 42 created                   "
    " 09:15:11.360 DEBUG c.e.o.OrderService$Vali… Checked 3 lines: [sku-1, sku-7, sku"
    " 09:15:12.004 WARN  c.e.p.PaymentClient      Retry 1/3 - gateway answered 503 (S"
    " 09:15:14.006 WARN  c.e.p.PaymentClient      Retry 2/3 - gateway answered 503 (S"
    " 09:15:18.011 ERROR c.e.p.PaymentClient      Payment for order 42 failed [+11]  "
    " 09:15:18.020 INFO  c.e.order.OrderService   Order 42 moved to PAYMENT_FAILED   "
    " 09:16:00.000 TRACE c.e.o.OrderCleanupJob    Tick                               "
    " 09:16:00.003 DEBUG c.e.o.OrderCleanupJob    Found 0 stale orders               "
    " 09:16:30.517 INFO  o.s.w.s.DispatcherServl… Completed initialization in 2 ms   "
    " 09:17:45.250 ERROR o.a.c.c.C.[.[.[.[dispat… Servlet.service() for servlet  [+6]"
    " 09:18:02.771 INFO  c.e.report.ReportService Report 2026-10 queued : 1284 rows  "
    " 09:18:09.903 WARN  c.e.report.ReportService Slow query (2314 ms): SELECT * FROM"
    " 09:19:40.118 INFO  c.e.o.OrderService$$Lam… Mapped 12 orders                   "
    " 09:20:00.000 INFO  c.e.shop.ShopApplication                                    "
    " 09:21:13.640 WARN  c.z.h.pool.HikariPool    HikariPool-1 - Thread starvation or"
    " 09:22:05.402 ERROR c.e.o.OrderController    Request failed [POST /api/ord [+10]"
    " 09:23:59.999 INFO  c.z.h.HikariDataSource   HikariPool-1 - Shutdown completed. "
    "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit            "
    "#);
}

#[test]
fn logback_at_120_by_30() {
    let screen = screen(Preset::Logback, "logback.log", 120, 30);
    assert_snapshot!(screen, @r#"
    "jlv ─ logback.log ─ 4.7 KB ─ Logback                                                                                    "
    "▶2026-10-07 09:14:02.118 INFO  com.example.shop.ShopApplication     Starting ShopApplication v1.4.2 using Java 21.0.4 wi"
    " 2026-10-07 09:14:02.121 DEBUG com.example.shop.ShopApplication     Running with Spring Boot v3.3.4, Spring v6.1.13     "
    " 2026-10-07 09:14:03.502 INFO  o.s.b.w.e.tomcat.TomcatWebServer     Tomcat initialized with port 8080 (http)            "
    " 2026-10-07 09:14:04.877 INFO  com.zaxxer.hikari.HikariDataSource   HikariPool-1 - Start completed.                     "
    " 2026-10-07 09:14:06.010 INFO  com.example.shop.ShopApplication     Started ShopApplication in 4.213 seconds (process ru"
    " 2026-10-07 09:15:11.342 INFO  com.example.order.OrderService       Order 42 created                                    "
    " 2026-10-07 09:15:11.360 DEBUG c.e.order.OrderService$Validator     Checked 3 lines: [sku-1, sku-7, sku-9]              "
    " 2026-10-07 09:15:12.004 WARN  com.example.payment.PaymentClient    Retry 1/3 - gateway answered 503 (Service Unavailabl"
    " 2026-10-07 09:15:14.006 WARN  com.example.payment.PaymentClient    Retry 2/3 - gateway answered 503 (Service Unavailabl"
    " 2026-10-07 09:15:18.011 ERROR com.example.payment.PaymentClient    Payment for order 42 failed [+11]                   "
    " 2026-10-07 09:15:18.020 INFO  com.example.order.OrderService       Order 42 moved to PAYMENT_FAILED                    "
    " 2026-10-07 09:16:00.000 TRACE com.example.order.OrderCleanupJob    Tick                                                "
    " 2026-10-07 09:16:00.003 DEBUG com.example.order.OrderCleanupJob    Found 0 stale orders                                "
    " 2026-10-07 09:16:30.517 INFO  o.s.web.servlet.DispatcherServlet    Completed initialization in 2 ms                    "
    " 2026-10-07 09:17:45.250 ERROR o.a.c.c.C.[.[.[.[dispatcherServlet]  Servlet.service() for servlet [dispatcherServle [+6]"
    " 2026-10-07 09:18:02.771 INFO  com.example.report.ReportService     Report 2026-10 queued : 1284 rows                   "
    " 2026-10-07 09:18:09.903 WARN  com.example.report.ReportService     Slow query (2314 ms): SELECT * FROM orders WHERE mon"
    " 2026-10-07 09:19:40.118 INFO  c.e.o.OrderService$$Lambda$1342/0x0… Mapped 12 orders                                    "
    " 2026-10-07 09:20:00.000 INFO  com.example.shop.ShopApplication                                                         "
    " 2026-10-07 09:21:13.640 WARN  com.zaxxer.hikari.pool.HikariPool    HikariPool-1 - Thread starvation or clock leap detec"
    " 2026-10-07 09:22:05.402 ERROR com.example.order.OrderController    Request failed [POST /api/orders] (user=anna) [+10] "
    " 2026-10-07 09:23:59.999 INFO  com.zaxxer.hikari.HikariDataSource   HikariPool-1 - Shutdown completed.                  "
    "                                                                                                                        "
    "                                                                                                                        "
    "                                                                                                                        "
    "                                                                                                                        "
    "                                                                                                                        "
    "                                                                                                                        "
    "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit                                                    "
    "#);
}

#[test]
fn logback_basic_at_80_by_24() {
    let screen = screen(Preset::LogbackBasic, "logback-basic.log", 80, 24);
    assert_snapshot!(screen, @r#"
    "jlv ─ logback-basic.log ─ 3.0 KB ─ Logback basic                                "
    "▶23:58:41.007 INFO  c.example.batch.BatchApp Starting nightly import            "
    " 23:58:41.093 DEBUG c.e.b.c.ImportConfig     Source directory: /data/inbox      "
    " 23:58:41.102 DEBUG c.e.b.c.ImportConfig     Batch size: 500                    "
    " 23:58:42.330 INFO  c.e.batch.ImportJob      Found 3 files to import            "
    " 23:58:42.331 INFO  c.e.batch.ImportJob      Importing customers-2026-10-07.csv "
    " 23:59:10.874 WARN  c.e.b.p.CsvRecordParser  Line 1207: unknown country code "XX"
    " 23:59:31.502 INFO  c.e.batch.ImportJob      customers-2026-10-07.csv done: 4821"
    " 23:59:31.510 INFO  c.e.batch.ImportJob      Importing orders-2026-10-07.csv (88"
    " 23:59:58.996 DEBUG c.e.b.p.CsvRecordParser… Flushed 500 rows [offset=120000]   "
    " 00:00:00.004 DEBUG c.e.b.p.CsvRecordParser… Flushed 500 rows [offset=120500]   "
    " 00:00:02.417 ERROR c.e.batch.ImportJob      orders-2026-10-07.csv failed  [+10]"
    " 00:00:02.420 INFO  c.e.batch.ImportJob      Moved orders-2026-10-07.csv to /dat"
    " 00:00:02.421 INFO  c.e.batch.ImportJob      Importing returns-2026-10-07.csv (1"
    " 00:00:03.008 INFO  c.e.batch.ImportJob      returns-2026-10-07.csv done: 311 ro"
    " 00:00:03.010 INFO  c.example.batch.BatchApp Import finished: 2 of 3 files (1 fa"
    " 00:00:03.011 WARN  c.example.batch.BatchApp Summary -- ok: 2, failed: 1 -- see "
    " 00:00:03.009 DEBUG c.e.batch.ImportJob      Released worker import-3           "
    " 00:00:03.150 INFO  c.e.batch.mail.Notifier  Sending report to ops@example.com  "
    " 00:00:04.902 ERROR c.e.batch.mail.Notifier  Could not send report [+7]         "
    " 00:00:04.903 TRACE c.e.batch.mail.Notifier                                     "
    " 00:00:05.000 INFO  c.example.batch.BatchApp Exit code 1                        "
    " 00:00:05.001 DEBUG c.example.batch.BatchApp Releasing lock /var/run/batch.lock "
    "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit            "
    "#);
}

#[test]
fn logback_basic_at_120_by_30() {
    let screen = screen(Preset::LogbackBasic, "logback-basic.log", 120, 30);
    assert_snapshot!(screen, @r#"
    "jlv ─ logback-basic.log ─ 3.0 KB ─ Logback basic                                                                        "
    "▶2026-10-07 23:58:41.007 INFO  com.example.batch.BatchApp           Starting nightly import                             "
    " 2026-10-07 23:58:41.093 DEBUG c.e.batch.config.ImportConfig        Source directory: /data/inbox                       "
    " 2026-10-07 23:58:41.102 DEBUG c.e.batch.config.ImportConfig        Batch size: 500                                     "
    " 2026-10-07 23:58:42.330 INFO  com.example.batch.ImportJob          Found 3 files to import                             "
    " 2026-10-07 23:58:42.331 INFO  com.example.batch.ImportJob          Importing customers-2026-10-07.csv (14 MB)          "
    " 2026-10-07 23:59:10.874 WARN  c.e.batch.parse.CsvRecordParser      Line 1207: unknown country code "XX" - using default"
    " 2026-10-07 23:59:31.502 INFO  com.example.batch.ImportJob          customers-2026-10-07.csv done: 48211 rows           "
    " 2026-10-07 23:59:31.510 INFO  com.example.batch.ImportJob          Importing orders-2026-10-07.csv (88 MB)             "
    " 2026-10-07 23:59:58.996 DEBUG c.e.b.parse.CsvRecordParser$Row      Flushed 500 rows [offset=120000]                    "
    " 2026-10-08 00:00:00.004 DEBUG c.e.b.parse.CsvRecordParser$Row      Flushed 500 rows [offset=120500]                    "
    " 2026-10-08 00:00:02.417 ERROR com.example.batch.ImportJob          orders-2026-10-07.csv failed at line 120731 [+10]   "
    " 2026-10-08 00:00:02.420 INFO  com.example.batch.ImportJob          Moved orders-2026-10-07.csv to /data/failed         "
    " 2026-10-08 00:00:02.421 INFO  com.example.batch.ImportJob          Importing returns-2026-10-07.csv (1 MB)             "
    " 2026-10-08 00:00:03.008 INFO  com.example.batch.ImportJob          returns-2026-10-07.csv done: 311 rows               "
    " 2026-10-08 00:00:03.010 INFO  com.example.batch.BatchApp           Import finished: 2 of 3 files (1 failed)            "
    " 2026-10-08 00:00:03.011 WARN  com.example.batch.BatchApp           Summary -- ok: 2, failed: 1 -- see /data/failed     "
    " 2026-10-08 00:00:03.009 DEBUG com.example.batch.ImportJob          Released worker import-3                            "
    " 2026-10-08 00:00:03.150 INFO  c.example.batch.mail.Notifier        Sending report to ops@example.com                   "
    " 2026-10-08 00:00:04.902 ERROR c.example.batch.mail.Notifier        Could not send report [+7]                          "
    " 2026-10-08 00:00:04.903 TRACE c.example.batch.mail.Notifier                                                            "
    " 2026-10-08 00:00:05.000 INFO  com.example.batch.BatchApp           Exit code 1                                         "
    " 2026-10-08 00:00:05.001 DEBUG com.example.batch.BatchApp           Releasing lock /var/run/batch.lock                  "
    "                                                                                                                        "
    "                                                                                                                        "
    "                                                                                                                        "
    "                                                                                                                        "
    "                                                                                                                        "
    "                                                                                                                        "
    "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit                                                    "
    "#);
}

#[test]
fn spring3_at_80_by_24() {
    let screen = screen(Preset::Spring3, "spring3.log", 80, 24);
    assert_snapshot!(screen, @r#"
    "jlv ─ spring3.log ─ 6.3 KB ─ Spring Boot 3                                      "
    "▶                                             [+9]                              "
    " 09:14:02.118 INFO  c.e.shop.ShopApplication Starting ShopApplication v1.4.2 usi"
    " 09:14:02.121 DEBUG c.e.shop.ShopApplication Running with Spring Boot v3.3.4, Sp"
    " 09:14:02.122 INFO  c.e.shop.ShopApplication The following 1 profile is active: "
    " 09:14:03.502 INFO  o.s.b.w.e.t.TomcatWebSe… Tomcat initialized with port 8080 ("
    " 09:14:03.517 INFO  o.a.c.c.StandardService  Starting service [Tomcat]          "
    " 09:14:03.518 INFO  o.a.c.c.StandardEngine   Starting Servlet engine: [Apache To"
    " 09:14:03.581 INFO  o.a.c.c.C.[.[.[/]        Initializing Spring embedded WebApp"
    " 09:14:04.877 INFO  c.z.h.HikariDataSource   HikariPool-1 - Start completed.    "
    " 09:14:05.920 WARN  JpaBaseConfiguration$Jp… spring.jpa.open-in-view is enabled "
    " 09:14:06.010 INFO  c.e.shop.ShopApplication Started ShopApplication in 4.213 se"
    " 09:15:11.330 INFO  o.a.c.c.C.[.[.[/]        Initializing Spring DispatcherServl"
    " 09:15:11.342 INFO  c.e.order.OrderService   Order 42 created : 3 lines         "
    " 09:15:12.004 WARN  c.e.p.PaymentClient      Retry 1/3 - gateway answered 503   "
    " 09:15:18.011 ERROR c.e.p.PaymentClient      Payment for order 42 failed [+11]  "
    " 09:16:00.000 TRACE derCleanupJob$$Lambda/0… Tick                               "
    " 09:16:30.517 INFO  o.s.w.s.DispatcherServl… Completed initialization in 2 ms   "
    " 09:17:45.250 ERROR o.a.c.c.C.[.[.[.[dispat… Servlet.service() for servlet  [+7]"
    " 09:18:02.771 INFO  c.e.r.ReportService$Exp… Report 2026-10 queued [1284 rows]  "
    " 09:20:00.000 INFO  c.e.shop.ShopApplication                                    "
    " 09:21:13.640 WARN  c.z.h.pool.HikariPool    HikariPool-1 - Thread starvation or"
    " 09:23:59.990 INFO  o.s.b.w.e.t.GracefulShu… Commencing graceful shutdown. Waiti"
    "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit            "
    "#);
}

#[test]
fn spring3_at_120_by_30() {
    let screen = screen(Preset::Spring3, "spring3.log", 120, 30);
    assert_snapshot!(screen, @r#"
    "jlv ─ spring3.log ─ 6.3 KB ─ Spring Boot 3                                                                              "
    "▶                                                                    [+9]                                               "
    " 2026-10-07 09:14:02.118 INFO  com.example.shop.ShopApplication     Starting ShopApplication v1.4.2 using Java 21.0.4 wi"
    " 2026-10-07 09:14:02.121 DEBUG com.example.shop.ShopApplication     Running with Spring Boot v3.3.4, Spring v6.1.13     "
    " 2026-10-07 09:14:02.122 INFO  com.example.shop.ShopApplication     The following 1 profile is active: "prod"           "
    " 2026-10-07 09:14:03.502 INFO  o.s.b.w.e.tomcat.TomcatWebServer     Tomcat initialized with port 8080 (http)            "
    " 2026-10-07 09:14:03.517 INFO  o.a.catalina.core.StandardService    Starting service [Tomcat]                           "
    " 2026-10-07 09:14:03.518 INFO  o.a.catalina.core.StandardEngine     Starting Servlet engine: [Apache Tomcat/10.1.30]    "
    " 2026-10-07 09:14:03.581 INFO  o.a.c.c.C.[Tomcat].[localhost].[/]   Initializing Spring embedded WebApplicationContext  "
    " 2026-10-07 09:14:04.877 INFO  com.zaxxer.hikari.HikariDataSource   HikariPool-1 - Start completed.                     "
    " 2026-10-07 09:14:05.920 WARN  JpaBaseConfiguration$JpaWebConfigur… spring.jpa.open-in-view is enabled by default. There"
    " 2026-10-07 09:14:06.010 INFO  com.example.shop.ShopApplication     Started ShopApplication in 4.213 seconds (process ru"
    " 2026-10-07 09:15:11.330 INFO  o.a.c.c.C.[Tomcat].[localhost].[/]   Initializing Spring DispatcherServlet 'dispatcherSer"
    " 2026-10-07 09:15:11.342 INFO  com.example.order.OrderService       Order 42 created : 3 lines                          "
    " 2026-10-07 09:15:12.004 WARN  com.example.payment.PaymentClient    Retry 1/3 - gateway answered 503                    "
    " 2026-10-07 09:15:18.011 ERROR com.example.payment.PaymentClient    Payment for order 42 failed [+11]                   "
    " 2026-10-07 09:16:00.000 TRACE derCleanupJob$$Lambda/0x000071c4e44… Tick                                                "
    " 2026-10-07 09:16:30.517 INFO  o.s.web.servlet.DispatcherServlet    Completed initialization in 2 ms                    "
    " 2026-10-07 09:17:45.250 ERROR o.a.c.c.C.[.[.[.[dispatcherServlet]  Servlet.service() for servlet [dispatcherServle [+7]"
    " 2026-10-07 09:18:02.771 INFO  c.e.report.ReportService$Exporter    Report 2026-10 queued [1284 rows]                   "
    " 2026-10-07 09:20:00.000 INFO  com.example.shop.ShopApplication                                                         "
    " 2026-10-07 09:21:13.640 WARN  com.zaxxer.hikari.pool.HikariPool    HikariPool-1 - Thread starvation or clock leap detec"
    " 2026-10-07 09:23:59.990 INFO  o.s.b.w.e.tomcat.GracefulShutdown    Commencing graceful shutdown. Waiting for active req"
    " 2026-10-07 09:23:59.999 INFO  com.zaxxer.hikari.HikariDataSource   HikariPool-1 - Shutdown completed.                  "
    " 2026-10-07 09:30:12.004 INFO  com.example.shop.ShopApplication     Starting ShopApplication v1.4.3 using Java 21.0.4 wi"
    " 2026-10-07 09:30:16.220 INFO  com.example.shop.ShopApplication     Started ShopApplication in 4.402 seconds (process ru"
    " 2026-10-07 09:31:02.401 INFO  o.s.web.servlet.DispatcherServlet    Completed initialization in 2 ms                    "
    " 2026-10-07 09:31:02.415 INFO  com.example.order.OrderService       Order 44 created                                    "
    " 2026-10-07 09:31:02.590 ERROR com.example.payment.PaymentClient    Payment for order 44 failed [+5]                    "
    "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit                                                    "
    "#);
}

#[test]
fn spring2_at_80_by_24() {
    let screen = screen(Preset::Spring2, "spring2.log", 80, 24);
    assert_snapshot!(screen, @r#"
    "jlv ─ spring2.log ─ 4.2 KB ─ Spring Boot 2                                      "
    "▶09:14:02.118 INFO  c.e.shop.ShopApplication Starting ShopApplication v1.2.0 usi"
    " 09:14:02.121 DEBUG c.e.shop.ShopApplication Running with Spring Boot v2.7.18, S"
    " 09:14:02.122 INFO  c.e.shop.ShopApplication No active profile set, falling back"
    " 09:14:03.502 INFO  o.s.b.w.e.t.TomcatWebSe… Tomcat initialized with port(s): 80"
    " 09:14:03.517 INFO  o.a.c.c.StandardService  Starting service [Tomcat]          "
    " 09:14:03.518 INFO  o.a.c.c.StandardEngine   Starting Servlet engine: [Apache To"
    " 09:14:03.581 INFO  o.a.c.c.C.[.[.[/]        Initializing Spring embedded WebApp"
    " 09:14:03.582 INFO  w.s.c.ServletWebServerA… Root WebApplicationContext: initial"
    " 09:14:04.877 INFO  c.z.h.HikariDataSource   HikariPool-1 - Start completed.    "
    " 09:14:06.010 INFO  c.e.shop.ShopApplication Started ShopApplication in 4.213 se"
    " 09:15:11.342 INFO  c.e.order.OrderService   Order 42 created                   "
    " 09:15:11.360 DEBUG c.e.o.OrderService$Vali… Checked 3 lines: [sku-1, sku-7, sku"
    " 09:15:12.004 WARN  c.e.p.PaymentClient      Retry 1/3 - gateway answered 503 : "
    " 09:15:18.011 ERROR c.e.p.PaymentClient      Payment for order 42 failed [+11]  "
    " 09:16:00.000 TRACE c.e.o.OrderCleanupJob    Tick                               "
    " 09:17:45.250 ERROR o.a.c.c.C.[.[.[.[dispat… Servlet.service() for servlet  [+7]"
    " 09:18:02.771 INFO  c.e.report.ReportService Report 2026-10 queued (1284 rows)  "
    " 09:19:40.118 INFO  erService$$Lambda$1342/… Mapped 12 orders                   "
    " 09:20:00.000 INFO  c.e.shop.ShopApplication                                    "
    " 09:21:13.640 WARN  c.z.h.pool.HikariPool    HikariPool-1 - Thread starvation or"
    " 09:23:59.999 INFO  c.z.h.HikariDataSource   HikariPool-1 - Shutdown completed. "
    "                                                                                "
    "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit            "
    "#);
}

#[test]
fn spring2_at_120_by_30() {
    let screen = screen(Preset::Spring2, "spring2.log", 120, 30);
    assert_snapshot!(screen, @r#"
    "jlv ─ spring2.log ─ 4.2 KB ─ Spring Boot 2                                                                              "
    "▶2026-10-07 09:14:02.118 INFO  com.example.shop.ShopApplication     Starting ShopApplication v1.2.0 using Java 11.0.24 o"
    " 2026-10-07 09:14:02.121 DEBUG com.example.shop.ShopApplication     Running with Spring Boot v2.7.18, Spring v5.3.31    "
    " 2026-10-07 09:14:02.122 INFO  com.example.shop.ShopApplication     No active profile set, falling back to 1 default pro"
    " 2026-10-07 09:14:03.502 INFO  o.s.b.w.e.tomcat.TomcatWebServer     Tomcat initialized with port(s): 8080 (http)        "
    " 2026-10-07 09:14:03.517 INFO  o.a.catalina.core.StandardService    Starting service [Tomcat]                           "
    " 2026-10-07 09:14:03.518 INFO  o.a.catalina.core.StandardEngine     Starting Servlet engine: [Apache Tomcat/9.0.83]     "
    " 2026-10-07 09:14:03.581 INFO  o.a.c.c.C.[Tomcat].[localhost].[/]   Initializing Spring embedded WebApplicationContext  "
    " 2026-10-07 09:14:03.582 INFO  w.s.c.ServletWebServerApplicationCo… Root WebApplicationContext: initialization completed"
    " 2026-10-07 09:14:04.877 INFO  com.zaxxer.hikari.HikariDataSource   HikariPool-1 - Start completed.                     "
    " 2026-10-07 09:14:06.010 INFO  com.example.shop.ShopApplication     Started ShopApplication in 4.213 seconds (JVM runnin"
    " 2026-10-07 09:15:11.342 INFO  com.example.order.OrderService       Order 42 created                                    "
    " 2026-10-07 09:15:11.360 DEBUG c.e.order.OrderService$Validator     Checked 3 lines: [sku-1, sku-7, sku-9]              "
    " 2026-10-07 09:15:12.004 WARN  com.example.payment.PaymentClient    Retry 1/3 - gateway answered 503 : Service Unavailab"
    " 2026-10-07 09:15:18.011 ERROR com.example.payment.PaymentClient    Payment for order 42 failed [+11]                   "
    " 2026-10-07 09:16:00.000 TRACE c.e.order.OrderCleanupJob            Tick                                                "
    " 2026-10-07 09:17:45.250 ERROR o.a.c.c.C.[.[.[.[dispatcherServlet]  Servlet.service() for servlet [dispatcherServle [+7]"
    " 2026-10-07 09:18:02.771 INFO  c.example.report.ReportService       Report 2026-10 queued (1284 rows)                   "
    " 2026-10-07 09:19:40.118 INFO  erService$$Lambda$1342/0x0000000801… Mapped 12 orders                                    "
    " 2026-10-07 09:20:00.000 INFO  com.example.shop.ShopApplication                                                         "
    " 2026-10-07 09:21:13.640 WARN  com.zaxxer.hikari.pool.HikariPool    HikariPool-1 - Thread starvation or clock leap detec"
    " 2026-10-07 09:23:59.999 INFO  com.zaxxer.hikari.HikariDataSource   HikariPool-1 - Shutdown completed.                  "
    "                                                                                                                        "
    "                                                                                                                        "
    "                                                                                                                        "
    "                                                                                                                        "
    "                                                                                                                        "
    "                                                                                                                        "
    "                                                                                                                        "
    "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit                                                    "
    "#);
}

#[test]
fn wildfly_at_80_by_24() {
    let screen = screen(Preset::Wildfly, "wildfly.log", 80, 24);
    assert_snapshot!(screen, @r#"
    "jlv ─ wildfly.log ─ 5.3 KB ─ WildFly                                            "
    "▶09:13:51.202 INFO  org.jboss.modules        JBoss Modules version 2.1.5.Final  "
    " 09:13:51.804 INFO  org.jboss.msc            JBoss MSC version 1.5.5.Final      "
    " 09:13:51.813 INFO  org.jboss.threads        JBoss Threads version 2.4.0.Final  "
    " 09:13:51.980 INFO  org.jboss.as             WFLYSRV0049: WildFly Full 33.0.0.Fi"
    " 09:13:53.114 INFO  o.w.extension.undertow   WFLYUT0006: Undertow HTTP listener "
    " 09:13:53.640 WARN  org.jboss.as.txn         WFLYTX0013: The node-identifier att"
    " 09:13:54.021 INFO  o.j.a.c.s.datasources    WFLYJCA0001: Bound data source [jav"
    " 09:13:55.388 INFO  o.j.as.server.deployment WFLYSRV0027: Starting deployment of"
    " 09:13:57.912 INFO  org.hibernate.Version    HHH000412: Hibernate ORM core versi"
    " 09:13:59.103 INFO  o.w.extension.undertow   WFLYUT0021: Registered web context:"
    " 09:13:59.170 INFO  org.jboss.as.server      WFLYSRV0010: Deployed "shop.war" (r"
    " 09:13:59.240 INFO  org.jboss.as.server      WFLYSRV0025: WildFly Full 33.0.0.Fi"
    " 09:15:11.342 INFO  c.e.order.OrderService   Order 42 created                   "
    " 09:15:11.360 DEBUG c.e.o.OrderService$Vali… Checked 3 lines: [sku-1, sku-7, sku"
    " 09:15:12.004 WARN  c.e.p.PaymentClient      Retry 1/3 - gateway answered 503 (S"
    " 09:15:18.011 ERROR c.e.p.PaymentClient      Payment for order 42 failed: c [+9]"
    " 09:16:00.000 TRACE c.e.o.OrderCleanupJob    Tick                               "
    " 09:17:45.250 ERROR io.undertow.request      UT005023: Exception handling  [+11]"
    " 09:18:02.771 INFO  stdout                   Report 2026-10 queued - 1284 rows  "
    " 09:18:02.772 INFO  stdout                                                      "
    " 09:18:09.903 ERROR stderr                   java.sql.SQLTimeoutException: query"
    " 09:18:09.904 ERROR stderr                       at deployment.shop.war//com.exa"
    "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit            "
    "#);
}

#[test]
fn wildfly_at_120_by_30() {
    let screen = screen(Preset::Wildfly, "wildfly.log", 120, 30);
    assert_snapshot!(screen, @r#"
    "jlv ─ wildfly.log ─ 5.3 KB ─ WildFly                                                                                    "
    "▶2026-10-07 09:13:51.202 INFO  org.jboss.modules                    JBoss Modules version 2.1.5.Final                   "
    " 2026-10-07 09:13:51.804 INFO  org.jboss.msc                        JBoss MSC version 1.5.5.Final                       "
    " 2026-10-07 09:13:51.813 INFO  org.jboss.threads                    JBoss Threads version 2.4.0.Final                   "
    " 2026-10-07 09:13:51.980 INFO  org.jboss.as                         WFLYSRV0049: WildFly Full 33.0.0.Final (WildFly Core"
    " 2026-10-07 09:13:53.114 INFO  org.wildfly.extension.undertow       WFLYUT0006: Undertow HTTP listener default listening"
    " 2026-10-07 09:13:53.640 WARN  org.jboss.as.txn                     WFLYTX0013: The node-identifier attribute on the /su"
    " 2026-10-07 09:13:54.021 INFO  o.j.a.c.subsystems.datasources       WFLYJCA0001: Bound data source [java:jboss/datasourc"
    " 2026-10-07 09:13:55.388 INFO  org.jboss.as.server.deployment       WFLYSRV0027: Starting deployment of "shop.war" (runt"
    " 2026-10-07 09:13:57.912 INFO  org.hibernate.Version                HHH000412: Hibernate ORM core version 6.4.4.Final   "
    " 2026-10-07 09:13:59.103 INFO  org.wildfly.extension.undertow       WFLYUT0021: Registered web context: '/shop' for serv"
    " 2026-10-07 09:13:59.170 INFO  org.jboss.as.server                  WFLYSRV0010: Deployed "shop.war" (runtime-name : "sh"
    " 2026-10-07 09:13:59.240 INFO  org.jboss.as.server                  WFLYSRV0025: WildFly Full 33.0.0.Final (WildFly Core"
    " 2026-10-07 09:15:11.342 INFO  com.example.order.OrderService       Order 42 created                                    "
    " 2026-10-07 09:15:11.360 DEBUG c.e.order.OrderService$Validator     Checked 3 lines: [sku-1, sku-7, sku-9]              "
    " 2026-10-07 09:15:12.004 WARN  com.example.payment.PaymentClient    Retry 1/3 - gateway answered 503 (Service Unavailabl"
    " 2026-10-07 09:15:18.011 ERROR com.example.payment.PaymentClient    Payment for order 42 failed: com.example.paymen [+9]"
    " 2026-10-07 09:16:00.000 TRACE com.example.order.OrderCleanupJob    Tick                                                "
    " 2026-10-07 09:17:45.250 ERROR io.undertow.request                  UT005023: Exception handling request to /shop/ [+11]"
    " 2026-10-07 09:18:02.771 INFO  stdout                               Report 2026-10 queued - 1284 rows                   "
    " 2026-10-07 09:18:02.772 INFO  stdout                                                                                   "
    " 2026-10-07 09:18:09.903 ERROR stderr                               java.sql.SQLTimeoutException: query timed out (2314 "
    " 2026-10-07 09:18:09.904 ERROR stderr                                   at deployment.shop.war//com.example.report.Repor"
    " 2026-10-07 09:19:40.118 INFO  c.e.o.OrderService$$Lambda/0x000071… Mapped 12 orders                                    "
    " 2026-10-07 09:21:13.640 WARN  com.arjuna.ats.arjuna                ARJUNA012117: TransactionReaper::check timeout for T"
    " 2026-10-07 09:21:13.651 WARN  com.arjuna.ats.arjuna                ARJUNA012095: Abort of action id 0:ffff7f000001:4b1a"
    " 2026-10-07 09:22:30.004 INFO  org.jboss.as.server                  WFLYSRV0016: Replaced deployment "shop.war" with dep"
    " 2026-10-07 09:23:59.990 INFO  org.jboss.as                         WFLYSRV0050: WildFly Full 33.0.0.Final (WildFly Core"
    "                                                                                                                        "
    "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit                                                    "
    "#);
}

#[test]
fn wildfly_console_at_80_by_24() {
    let screen = screen(Preset::WildflyConsole, "wildfly-console.log", 80, 24);
    assert_snapshot!(screen, @r#"
    "jlv ─ wildfly-console.log ─ 5.6 KB ─ WildFly console                            "
    "▶                                            ============================= [+11]"
    " 23:59:56.102 INFO  org.jboss.modules        JBoss Modules version 2.1.5.Final  "
    " 23:59:56.690 INFO  org.jboss.msc            JBoss MSC version 1.5.5.Final      "
    " 23:59:56.698 INFO  org.jboss.threads        JBoss Threads version 2.4.0.Final  "
    " 23:59:56.871 INFO  org.jboss.as             WFLYSRV0049: WildFly Full 33.0.0.Fi"
    " 23:59:57.630 INFO  org.jboss.as.server      WFLYSRV0039: Creating http manageme"
    " 23:59:58.014 WARN  org.jboss.as.txn         WFLYTX0013: The node-identifier att"
    " 23:59:58.412 INFO  o.w.extension.undertow   WFLYUT0006: Undertow HTTP listener "
    " 23:59:59.021 INFO  o.j.a.c.s.datasources    WFLYJCA0001: Bound data source [jav"
    " 23:59:59.388 INFO  o.j.as.server.deployment WFLYSRV0027: Starting deployment of"
    " 00:00:00.912 INFO  org.jboss.weld.deployer  WFLYWELD0003: Processing weld deplo"
    " 00:00:01.233 ERROR o.jboss.msc.service.fail MSC000001: Failed to start se [+13]"
    " 00:00:01.301 ERROR o.j.a.c.management-oper… WFLYCTL0013: Operation ("deplo [+2]"
    " 00:00:01.340 INFO  org.jboss.as.server      WFLYSRV0010: Deployed "legacy.war" "
    " 00:00:01.338 INFO  org.jboss.weld.Version   WELD-000900: 5.1.2 (Final)         "
    " 00:00:01.342 INFO  org.jboss.as.controller  WFLYCTL0183: Service status re [+1]"
    " 00:00:01.410 INFO  org.jboss.as.server      WFLYSRV0212: Resuming server       "
    " 00:00:01.413 INFO  org.jboss.as             WFLYSRV0060: Http management interf"
    " 00:00:01.415 ERROR org.jboss.as             WFLYSRV0026: WildFly Full 33.0.0.Fi"
    " 00:00:14.880 INFO  stdout                   Hello from (legacy) [servlet] - pat"
    " 00:00:15.002 DEBUG io.undertow.request.io   UT005013: An IOException occurred  "
    " 00:00:20.117 INFO  stdout                                                      "
    "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit            "
    "#);
}

#[test]
fn wildfly_console_at_120_by_30() {
    let screen = screen(Preset::WildflyConsole, "wildfly-console.log", 120, 30);
    assert_snapshot!(screen, @r#"
    "jlv ─ wildfly-console.log ─ 5.6 KB ─ WildFly console                                                                    "
    "▶                                                                   ============================================== [+11]"
    " 2026-10-07 23:59:56.102 INFO  org.jboss.modules                    JBoss Modules version 2.1.5.Final                   "
    " 2026-10-07 23:59:56.690 INFO  org.jboss.msc                        JBoss MSC version 1.5.5.Final                       "
    " 2026-10-07 23:59:56.698 INFO  org.jboss.threads                    JBoss Threads version 2.4.0.Final                   "
    " 2026-10-07 23:59:56.871 INFO  org.jboss.as                         WFLYSRV0049: WildFly Full 33.0.0.Final (WildFly Core"
    " 2026-10-07 23:59:57.630 INFO  org.jboss.as.server                  WFLYSRV0039: Creating http management service using "
    " 2026-10-07 23:59:58.014 WARN  org.jboss.as.txn                     WFLYTX0013: The node-identifier attribute on the /su"
    " 2026-10-07 23:59:58.412 INFO  org.wildfly.extension.undertow       WFLYUT0006: Undertow HTTP listener default listening"
    " 2026-10-07 23:59:59.021 INFO  o.j.a.c.subsystems.datasources       WFLYJCA0001: Bound data source [java:jboss/datasourc"
    " 2026-10-07 23:59:59.388 INFO  org.jboss.as.server.deployment       WFLYSRV0027: Starting deployment of "legacy.war" (ru"
    " 2026-10-08 00:00:00.912 INFO  org.jboss.weld.deployer              WFLYWELD0003: Processing weld deployment legacy.war "
    " 2026-10-08 00:00:01.233 ERROR org.jboss.msc.service.fail           MSC000001: Failed to start service jboss.deplo [+13]"
    " 2026-10-08 00:00:01.301 ERROR o.j.a.c.management-operation         WFLYCTL0013: Operation ("deploy") failed - addr [+2]"
    " 2026-10-08 00:00:01.340 INFO  org.jboss.as.server                  WFLYSRV0010: Deployed "legacy.war" (runtime-name : ""
    " 2026-10-08 00:00:01.338 INFO  org.jboss.weld.Version               WELD-000900: 5.1.2 (Final)                          "
    " 2026-10-08 00:00:01.342 INFO  org.jboss.as.controller              WFLYCTL0183: Service status report [+1]             "
    " 2026-10-08 00:00:01.410 INFO  org.jboss.as.server                  WFLYSRV0212: Resuming server                        "
    " 2026-10-08 00:00:01.413 INFO  org.jboss.as                         WFLYSRV0060: Http management interface listening on "
    " 2026-10-08 00:00:01.415 ERROR org.jboss.as                         WFLYSRV0026: WildFly Full 33.0.0.Final (WildFly Core"
    " 2026-10-08 00:00:14.880 INFO  stdout                               Hello from (legacy) [servlet] - path /legacy : ok   "
    " 2026-10-08 00:00:15.002 DEBUG io.undertow.request.io               UT005013: An IOException occurred                   "
    " 2026-10-08 00:00:20.117 INFO  stdout                                                                                   "
    " 2026-10-08 00:01:13.640 WARN  com.arjuna.ats.arjuna                ARJUNA012117: TransactionReaper::check timeout for T"
    " 2026-10-08 00:02:00.000 INFO  org.jboss.as.server                  WFLYSRV0220: Server shutdown has been requested via "
    " 2026-10-08 00:02:00.145 INFO  org.jboss.as                         WFLYSRV0050: WildFly Full 33.0.0.Final (WildFly Core"
    "                                                                                                                        "
    "                                                                                                                        "
    "                                                                                                                        "
    "j/k Move  PgUp/PgDn Page  Ctrl-u/d Half page  g/G Top/Bottom  q Quit                                                    "
    "#);
}
