//! The format presets against the fixture logs in `tests/fixtures`, one per preset.

use std::ops::Range;

use insta::assert_snapshot;
use vilog::format::{EntryHead, Format, strip_ansi};

mod common;

use common::{built_in, fixture, fixtures, lines};

/// The fixture's lines without line endings and ANSI-stripped, as the parser gets
/// them.
fn stripped_lines(name: &str) -> Vec<Vec<u8>> {
    lines(&fixture(name))
        .into_iter()
        .map(|line| strip_ansi(line).into_owned())
        .collect()
}

fn parse(preset: &'static Format, line: &[u8]) -> Option<EntryHead> {
    preset
        .entry_parser()
        .unwrap_or_else(|| panic!("{preset:?} has no entry parser"))
        .parse(line)
}

fn text(line: &[u8], span: Range<usize>) -> String {
    String::from_utf8_lossy(&line[span]).replace('\t', "\\t")
}

/// What `preset` makes of the fixture `name`. A line that starts an entry shows as
/// `timestamp | level | thread | logger | message`, a run of N lines that don't as
/// `+N`. Tabs show as `\t`.
fn outline(preset: &'static Format, name: &str) -> String {
    let mut out = Vec::new();
    let mut run = 0;
    for line in stripped_lines(name) {
        let Some(head) = parse(preset, &line) else {
            run += 1;
            continue;
        };
        if run > 0 {
            out.push(format!("+{run}"));
            run = 0;
        }
        let columns = [head.timestamp, head.level, head.thread, head.logger]
            .map(|span| span.map_or_else(String::new, |span| text(&line, span)))
            .join(" | ");
        let message = text(&line, head.message);
        if message.is_empty() {
            out.push(format!("{columns} |"));
        } else {
            out.push(format!("{columns} | {message}"));
        }
    }
    if run > 0 {
        out.push(format!("+{run}"));
    }
    out.join("\n")
}

#[test]
fn finds_every_entry_of_the_logback_fixture() {
    assert_snapshot!(outline(built_in("logback"), "logback.log"), @r#"
    2026-10-07 09:14:02.118 | INFO | main | com.example.shop.ShopApplication | Starting ShopApplication v1.4.2 using Java 21.0.4 with PID 48213
    2026-10-07 09:14:02.121 | DEBUG | main | com.example.shop.ShopApplication | Running with Spring Boot v3.3.4, Spring v6.1.13
    2026-10-07 09:14:03.502 | INFO | main | o.s.b.w.e.tomcat.TomcatWebServer | Tomcat initialized with port 8080 (http)
    2026-10-07 09:14:04.877 | INFO | main | com.zaxxer.hikari.HikariDataSource | HikariPool-1 - Start completed.
    2026-10-07 09:14:06.010 | INFO | main | com.example.shop.ShopApplication | Started ShopApplication in 4.213 seconds (process running for 4.9)
    2026-10-07 09:15:11.342 | INFO | http-nio-8080-exec-3 | com.example.order.OrderService | Order 42 created
    2026-10-07 09:15:11.360 | DEBUG | http-nio-8080-exec-3 | c.e.order.OrderService$Validator | Checked 3 lines: [sku-1, sku-7, sku-9]
    2026-10-07 09:15:12.004 | WARN | pool-1-thread-2 | com.example.payment.PaymentClient | Retry 1/3 - gateway answered 503 (Service Unavailable)
    2026-10-07 09:15:14.006 | WARN | pool-1-thread-2 | com.example.payment.PaymentClient | Retry 2/3 - gateway answered 503 (Service Unavailable)
    2026-10-07 09:15:18.011 | ERROR | pool-1-thread-2 | com.example.payment.PaymentClient | Payment for order 42 failed
    +11
    2026-10-07 09:15:18.020 | INFO | pool-1-thread-2 | com.example.order.OrderService | Order 42 moved to PAYMENT_FAILED
    2026-10-07 09:16:00.000 | TRACE | scheduling-1 | com.example.order.OrderCleanupJob | Tick
    2026-10-07 09:16:00.003 | DEBUG | scheduling-1 | com.example.order.OrderCleanupJob | Found 0 stale orders
    2026-10-07 09:16:30.517 | INFO | RMI TCP Connection(2)-127.0.0.1 | o.s.web.servlet.DispatcherServlet | Completed initialization in 2 ms
    2026-10-07 09:17:45.250 | ERROR | http-nio-8080-exec-7 | o.a.c.c.C.[.[.[/].[dispatcherServlet] | Servlet.service() for servlet [dispatcherServlet] threw exception
    +6
    2026-10-07 09:18:02.771 | INFO | ForkJoinPool.commonPool-worker-1 | com.example.report.ReportService | Report 2026-10 queued : 1284 rows
    2026-10-07 09:18:09.903 | WARN | ForkJoinPool.commonPool-worker-1 | com.example.report.ReportService | Slow query (2314 ms): SELECT * FROM orders WHERE month = ?
    2026-10-07 09:19:40.118 | INFO | http-nio-8080-exec-1 | c.e.order.OrderService$$Lambda$1342/0x00000008014c2a38 | Mapped 12 orders
    2026-10-07 09:20:00.000 | INFO | main | com.example.shop.ShopApplication |
    2026-10-07 09:21:13.640 | WARN | HikariPool-1 housekeeper | com.zaxxer.hikari.pool.HikariPool | HikariPool-1 - Thread starvation or clock leap detected (housekeeper delta=1m2s).
    2026-10-07 09:22:05.402 | ERROR | http-nio-8080-exec-9 | com.example.order.OrderController | Request failed [POST /api/orders] (user=anna)
    +10
    2026-10-07 09:23:59.999 | INFO | SpringApplicationShutdownHook | com.zaxxer.hikari.HikariDataSource | HikariPool-1 - Shutdown completed.
    "#);
}

#[test]
fn finds_every_entry_of_the_logback_basic_fixture() {
    assert_snapshot!(outline(built_in("logback-basic"), "logback-basic.log"), @r#"
    23:58:41.007 | INFO | main | com.example.batch.BatchApp | Starting nightly import
    23:58:41.093 | DEBUG | main | c.e.batch.config.ImportConfig | Source directory: /data/inbox
    23:58:41.102 | DEBUG | main | c.e.batch.config.ImportConfig | Batch size: 500
    23:58:42.330 | INFO | main | com.example.batch.ImportJob | Found 3 files to import
    23:58:42.331 | INFO | import-1 | com.example.batch.ImportJob | Importing customers-2026-10-07.csv (14 MB)
    23:59:10.874 | WARN | import-1 | c.e.batch.parse.CsvRecordParser | Line 1207: unknown country code "XX" - using default
    23:59:31.502 | INFO | import-1 | com.example.batch.ImportJob | customers-2026-10-07.csv done: 48211 rows
    23:59:31.510 | INFO | import-2 | com.example.batch.ImportJob | Importing orders-2026-10-07.csv (88 MB)
    23:59:58.996 | DEBUG | import-2 | c.e.b.parse.CsvRecordParser$Row | Flushed 500 rows [offset=120000]
    00:00:00.004 | DEBUG | import-2 | c.e.b.parse.CsvRecordParser$Row | Flushed 500 rows [offset=120500]
    00:00:02.417 | ERROR | import-2 | com.example.batch.ImportJob | orders-2026-10-07.csv failed at line 120731
    +10
    00:00:02.420 | INFO | import-2 | com.example.batch.ImportJob | Moved orders-2026-10-07.csv to /data/failed
    00:00:02.421 | INFO | import-3 | com.example.batch.ImportJob | Importing returns-2026-10-07.csv (1 MB)
    00:00:03.008 | INFO | import-3 | com.example.batch.ImportJob | returns-2026-10-07.csv done: 311 rows
    00:00:03.010 | INFO | main | com.example.batch.BatchApp | Import finished: 2 of 3 files (1 failed)
    00:00:03.011 | WARN | main | com.example.batch.BatchApp | Summary -- ok: 2, failed: 1 -- see /data/failed
    00:00:03.009 | DEBUG | import-3 | com.example.batch.ImportJob | Released worker import-3
    00:00:03.150 | INFO | Thread-0 | c.example.batch.mail.Notifier | Sending report to ops@example.com
    00:00:04.902 | ERROR | Thread-0 | c.example.batch.mail.Notifier | Could not send report
    +7
    00:00:04.903 | TRACE | Thread-0 | c.example.batch.mail.Notifier |
    00:00:05.000 | INFO | main | com.example.batch.BatchApp | Exit code 1
    00:00:05.001 | DEBUG | Shutdown hook | com.example.batch.BatchApp | Releasing lock /var/run/batch.lock
    "#);
}

#[test]
fn finds_every_entry_of_the_spring3_fixture() {
    assert_snapshot!(outline(built_in("spring3"), "spring3.log"), @r#"
    +10
    2026-10-07T09:14:02.118+02:00 | INFO | main | com.example.shop.ShopApplication | Starting ShopApplication v1.4.2 using Java 21.0.4 with PID 48213 (/opt/shop/shop.jar started by shop in /opt/shop)
    2026-10-07T09:14:02.121+02:00 | DEBUG | main | com.example.shop.ShopApplication | Running with Spring Boot v3.3.4, Spring v6.1.13
    2026-10-07T09:14:02.122+02:00 | INFO | main | com.example.shop.ShopApplication | The following 1 profile is active: "prod"
    2026-10-07T09:14:03.502+02:00 | INFO | main | o.s.b.w.embedded.tomcat.TomcatWebServer | Tomcat initialized with port 8080 (http)
    2026-10-07T09:14:03.517+02:00 | INFO | main | o.apache.catalina.core.StandardService | Starting service [Tomcat]
    2026-10-07T09:14:03.518+02:00 | INFO | main | o.apache.catalina.core.StandardEngine | Starting Servlet engine: [Apache Tomcat/10.1.30]
    2026-10-07T09:14:03.581+02:00 | INFO | main | o.a.c.c.C.[Tomcat].[localhost].[/] | Initializing Spring embedded WebApplicationContext
    2026-10-07T09:14:04.877+02:00 | INFO | main | com.zaxxer.hikari.HikariDataSource | HikariPool-1 - Start completed.
    2026-10-07T09:14:05.920+02:00 | WARN | main | JpaBaseConfiguration$JpaWebConfiguration | spring.jpa.open-in-view is enabled by default. Therefore, database queries may be performed during view rendering. Explicitly configure spring.jpa.open-in-view to disable this warning
    2026-10-07T09:14:06.010+02:00 | INFO | main | com.example.shop.ShopApplication | Started ShopApplication in 4.213 seconds (process running for 4.9)
    2026-10-07T09:15:11.330+02:00 | INFO | nio-8080-exec-1 | o.a.c.c.C.[Tomcat].[localhost].[/] | Initializing Spring DispatcherServlet 'dispatcherServlet'
    2026-10-07T09:15:11.342+02:00 | INFO | nio-8080-exec-3 | com.example.order.OrderService | Order 42 created : 3 lines
    2026-10-07T09:15:12.004+02:00 | WARN | pool-1-thread-2 | com.example.payment.PaymentClient | Retry 1/3 - gateway answered 503
    2026-10-07T09:15:18.011+02:00 | ERROR | pool-1-thread-2 | com.example.payment.PaymentClient | Payment for order 42 failed
    +11
    2026-10-07T09:16:00.000+02:00 | TRACE | scheduling-1 | derCleanupJob$$Lambda/0x000071c4e44c8a40 | Tick
    2026-10-07T09:16:30.517+02:00 | INFO | on(2)-127.0.0.1 | o.s.web.servlet.DispatcherServlet | Completed initialization in 2 ms
    2026-10-07T09:17:45.250+02:00 | ERROR | nio-8080-exec-7 | o.a.c.c.C.[.[.[/].[dispatcherServlet] | Servlet.service() for servlet [dispatcherServlet] in context with path [] threw exception [Request processing failed: java.lang.IllegalStateException: Order 43 is already closed] with root cause
    +7
    2026-10-07T09:18:02.771+02:00 | INFO | onPool-worker-1 | c.example.report.ReportService$Exporter | Report 2026-10 queued [1284 rows]
    2026-10-07T09:20:00.000+02:00 | INFO | main | com.example.shop.ShopApplication |
    2026-10-07T09:21:13.640+02:00 | WARN | l-1 housekeeper | com.zaxxer.hikari.pool.HikariPool | HikariPool-1 - Thread starvation or clock leap detected (housekeeper delta=1m2s).
    2026-10-07T09:23:59.990+02:00 | INFO | ionShutdownHook | o.s.b.w.e.tomcat.GracefulShutdown | Commencing graceful shutdown. Waiting for active requests to complete
    2026-10-07T09:23:59.999+02:00 | INFO | ionShutdownHook | com.zaxxer.hikari.HikariDataSource | HikariPool-1 - Shutdown completed.
    2026-10-07T09:30:12.004+02:00 | INFO | main | com.example.shop.ShopApplication | Starting ShopApplication v1.4.3 using Java 21.0.4 with PID 51877 (/opt/shop/shop.jar started by shop in /opt/shop)
    2026-10-07T09:30:16.220+02:00 | INFO | main | com.example.shop.ShopApplication | Started ShopApplication in 4.402 seconds (process running for 5.1)
    2026-10-07T09:31:02.401+02:00 | INFO | nio-8080-exec-2 | o.s.web.servlet.DispatcherServlet | Completed initialization in 2 ms
    2026-10-07T09:31:02.415+02:00 | INFO | nio-8080-exec-2 | com.example.order.OrderService | Order 44 created
    2026-10-07T09:31:02.590+02:00 | ERROR | nio-8080-exec-2 | com.example.payment.PaymentClient | Payment for order 44 failed
    +5
    2026-10-07T09:32:00.000+02:00 | TRACE | scheduling-1 | com.example.order.OrderCleanupJob | Tick
    "#);
}

#[test]
fn finds_every_entry_of_the_spring2_fixture() {
    assert_snapshot!(outline(built_in("spring2"), "spring2.log"), @r#"
    2026-10-07 09:14:02.118 | INFO | main | com.example.shop.ShopApplication | Starting ShopApplication v1.2.0 using Java 11.0.24 on shop-01 with PID 48213 (/opt/shop/shop.jar started by shop in /opt/shop)
    2026-10-07 09:14:02.121 | DEBUG | main | com.example.shop.ShopApplication | Running with Spring Boot v2.7.18, Spring v5.3.31
    2026-10-07 09:14:02.122 | INFO | main | com.example.shop.ShopApplication | No active profile set, falling back to 1 default profile: "default"
    2026-10-07 09:14:03.502 | INFO | main | o.s.b.w.embedded.tomcat.TomcatWebServer | Tomcat initialized with port(s): 8080 (http)
    2026-10-07 09:14:03.517 | INFO | main | o.apache.catalina.core.StandardService | Starting service [Tomcat]
    2026-10-07 09:14:03.518 | INFO | main | org.apache.catalina.core.StandardEngine | Starting Servlet engine: [Apache Tomcat/9.0.83]
    2026-10-07 09:14:03.581 | INFO | main | o.a.c.c.C.[Tomcat].[localhost].[/] | Initializing Spring embedded WebApplicationContext
    2026-10-07 09:14:03.582 | INFO | main | w.s.c.ServletWebServerApplicationContext | Root WebApplicationContext: initialization completed in 1358 ms
    2026-10-07 09:14:04.877 | INFO | main | com.zaxxer.hikari.HikariDataSource | HikariPool-1 - Start completed.
    2026-10-07 09:14:06.010 | INFO | main | com.example.shop.ShopApplication | Started ShopApplication in 4.213 seconds (JVM running for 4.9)
    2026-10-07 09:15:11.342 | INFO | nio-8080-exec-3 | com.example.order.OrderService | Order 42 created
    2026-10-07 09:15:11.360 | DEBUG | nio-8080-exec-3 | c.e.order.OrderService$Validator | Checked 3 lines: [sku-1, sku-7, sku-9]
    2026-10-07 09:15:12.004 | WARN | pool-1-thread-2 | com.example.payment.PaymentClient | Retry 1/3 - gateway answered 503 : Service Unavailable
    2026-10-07 09:15:18.011 | ERROR | pool-1-thread-2 | com.example.payment.PaymentClient | Payment for order 42 failed
    +11
    2026-10-07 09:16:00.000 | TRACE | scheduling-1 | c.e.order.OrderCleanupJob | Tick
    2026-10-07 09:17:45.250 | ERROR | nio-8080-exec-7 | o.a.c.c.C.[.[.[/].[dispatcherServlet] | Servlet.service() for servlet [dispatcherServlet] in context with path [] threw exception [Request processing failed; nested exception is java.lang.IllegalStateException: Order 43 is already closed] with root cause
    +7
    2026-10-07 09:18:02.771 | INFO | onPool-worker-1 | c.example.report.ReportService | Report 2026-10 queued (1284 rows)
    2026-10-07 09:19:40.118 | INFO | nio-8080-exec-1 | erService$$Lambda$1342/0x0000000801c2a38 | Mapped 12 orders
    2026-10-07 09:20:00.000 | INFO | main | com.example.shop.ShopApplication |
    2026-10-07 09:21:13.640 | WARN | l-1 housekeeper | com.zaxxer.hikari.pool.HikariPool | HikariPool-1 - Thread starvation or clock leap detected (housekeeper delta=1m2s).
    2026-10-07 09:23:59.999 | INFO | ionShutdownHook | com.zaxxer.hikari.HikariDataSource | HikariPool-1 - Shutdown completed.
    "#);
}

#[test]
fn finds_every_entry_of_the_wildfly_fixture() {
    assert_snapshot!(outline(built_in("wildfly"), "wildfly.log"), @r#"
    2026-10-07 09:13:51,202 | INFO | main | org.jboss.modules | JBoss Modules version 2.1.5.Final
    2026-10-07 09:13:51,804 | INFO | main | org.jboss.msc | JBoss MSC version 1.5.5.Final
    2026-10-07 09:13:51,813 | INFO | main | org.jboss.threads | JBoss Threads version 2.4.0.Final
    2026-10-07 09:13:51,980 | INFO | MSC service thread 1-2 | org.jboss.as | WFLYSRV0049: WildFly Full 33.0.0.Final (WildFly Core 25.0.0.Final) starting
    2026-10-07 09:13:53,114 | INFO | MSC service thread 1-6 | org.wildfly.extension.undertow | WFLYUT0006: Undertow HTTP listener default listening on 127.0.0.1:8080
    2026-10-07 09:13:53,640 | WARN | ServerService Thread Pool -- 76 | org.jboss.as.txn | WFLYTX0013: The node-identifier attribute on the /subsystem=transactions is set to the default value. This is a danger for environments running multiple servers. Please make sure the attribute value is unique.
    2026-10-07 09:13:54,021 | INFO | MSC service thread 1-4 | org.jboss.as.connector.subsystems.datasources | WFLYJCA0001: Bound data source [java:jboss/datasources/ShopDS]
    2026-10-07 09:13:55,388 | INFO | MSC service thread 1-3 | org.jboss.as.server.deployment | WFLYSRV0027: Starting deployment of "shop.war" (runtime-name: "shop.war")
    2026-10-07 09:13:57,912 | INFO | ServerService Thread Pool -- 81 | org.hibernate.Version | HHH000412: Hibernate ORM core version 6.4.4.Final
    2026-10-07 09:13:59,103 | INFO | ServerService Thread Pool -- 81 | org.wildfly.extension.undertow | WFLYUT0021: Registered web context: '/shop' for server 'default-server'
    2026-10-07 09:13:59,170 | INFO | Controller Boot Thread | org.jboss.as.server | WFLYSRV0010: Deployed "shop.war" (runtime-name : "shop.war")
    2026-10-07 09:13:59,240 | INFO | Controller Boot Thread | org.jboss.as.server | WFLYSRV0025: WildFly Full 33.0.0.Final (WildFly Core 25.0.0.Final) started in 8291ms - Started 512 of 734 services (339 services are lazy, passive or on-demand) - Server configuration file in use: standalone.xml
    2026-10-07 09:15:11,342 | INFO | default task-3 | com.example.order.OrderService | Order 42 created
    2026-10-07 09:15:11,360 | DEBUG | default task-3 | com.example.order.OrderService$Validator | Checked 3 lines: [sku-1, sku-7, sku-9]
    2026-10-07 09:15:12,004 | WARN | EE-ManagedExecutorService-default-Thread-2 | com.example.payment.PaymentClient | Retry 1/3 - gateway answered 503 (Service Unavailable)
    2026-10-07 09:15:18,011 | ERROR | EE-ManagedExecutorService-default-Thread-2 | com.example.payment.PaymentClient | Payment for order 42 failed: com.example.payment.PaymentException: gateway unavailable after 3 attempts
    +9
    2026-10-07 09:16:00,000 | TRACE | EE-ManagedScheduledExecutorService-default-Thread-1 | com.example.order.OrderCleanupJob | Tick
    2026-10-07 09:17:45,250 | ERROR | default task-7 | io.undertow.request | UT005023: Exception handling request to /shop/api/orders/43/close: jakarta.servlet.ServletException: java.lang.IllegalStateException: Order 43 is already closed
    +11
    2026-10-07 09:18:02,771 | INFO | default task-9 | stdout | Report 2026-10 queued - 1284 rows
    2026-10-07 09:18:02,772 | INFO | default task-9 | stdout |
    2026-10-07 09:18:09,903 | ERROR | default task-9 | stderr | java.sql.SQLTimeoutException: query timed out (2314 ms)
    2026-10-07 09:18:09,904 | ERROR | default task-9 | stderr | \tat deployment.shop.war//com.example.report.ReportDao.load(ReportDao.java:29)
    2026-10-07 09:19:40,118 | INFO | default task-1 | com.example.order.OrderService$$Lambda/0x000071c4e44c8a40 | Mapped 12 orders
    2026-10-07 09:21:13,640 | WARN | Transaction Reaper | com.arjuna.ats.arjuna | ARJUNA012117: TransactionReaper::check timeout for TX 0:ffff7f000001:4b1a3f2e:66f0a1c2:2e in state  RUN
    2026-10-07 09:21:13,651 | WARN | Transaction Reaper Worker 0 | com.arjuna.ats.arjuna | ARJUNA012095: Abort of action id 0:ffff7f000001:4b1a3f2e:66f0a1c2:2e invoked while multiple threads active within it.
    2026-10-07 09:22:30,004 | INFO | management-handler-thread - 2 | org.jboss.as.server | WFLYSRV0016: Replaced deployment "shop.war" with deployment "shop.war"
    2026-10-07 09:23:59,990 | INFO | MSC service thread 1-5 | org.jboss.as | WFLYSRV0050: WildFly Full 33.0.0.Final (WildFly Core 25.0.0.Final) stopped in 41ms
    "#);
}

#[test]
fn finds_every_entry_of_the_wildfly_console_fixture() {
    assert!(
        fixture("wildfly-console.log").contains(&0x1b),
        "the console fixture should be coloured"
    );
    assert_snapshot!(outline(built_in("wildfly-console"), "wildfly-console.log"), @r#"
    +12
    23:59:56,102 | INFO | main | org.jboss.modules | JBoss Modules version 2.1.5.Final
    23:59:56,690 | INFO | main | org.jboss.msc | JBoss MSC version 1.5.5.Final
    23:59:56,698 | INFO | main | org.jboss.threads | JBoss Threads version 2.4.0.Final
    23:59:56,871 | INFO | MSC service thread 1-2 | org.jboss.as | WFLYSRV0049: WildFly Full 33.0.0.Final (WildFly Core 25.0.0.Final) starting
    23:59:57,630 | INFO | Controller Boot Thread | org.jboss.as.server | WFLYSRV0039: Creating http management service using socket-binding (management-http)
    23:59:58,014 | WARN | ServerService Thread Pool -- 76 | org.jboss.as.txn | WFLYTX0013: The node-identifier attribute on the /subsystem=transactions is set to the default value. This is a danger for environments running multiple servers. Please make sure the attribute value is unique.
    23:59:58,412 | INFO | MSC service thread 1-6 | org.wildfly.extension.undertow | WFLYUT0006: Undertow HTTP listener default listening on 127.0.0.1:8080
    23:59:59,021 | INFO | MSC service thread 1-4 | org.jboss.as.connector.subsystems.datasources | WFLYJCA0001: Bound data source [java:jboss/datasources/ExampleDS]
    23:59:59,388 | INFO | MSC service thread 1-3 | org.jboss.as.server.deployment | WFLYSRV0027: Starting deployment of "legacy.war" (runtime-name: "legacy.war")
    00:00:00,912 | INFO | MSC service thread 1-1 | org.jboss.weld.deployer | WFLYWELD0003: Processing weld deployment legacy.war
    00:00:01,233 | ERROR | MSC service thread 1-1 | org.jboss.msc.service.fail | MSC000001: Failed to start service jboss.deployment.unit."legacy.war".POST_MODULE: org.jboss.msc.service.StartException in service jboss.deployment.unit."legacy.war".POST_MODULE: WFLYSRV0153: Failed to process phase POST_MODULE of deployment "legacy.war"
    +13
    00:00:01,301 | ERROR | Controller Boot Thread | org.jboss.as.controller.management-operation | WFLYCTL0013: Operation ("deploy") failed - address: ([("deployment" => "legacy.war")]) - failure description: {"WFLYCTL0080: Failed services" => {"jboss.deployment.unit.\"legacy.war\".POST_MODULE" => "WFLYSRV0153: Failed to process phase POST_MODULE of deployment \"legacy.war\"
    +2
    00:00:01,340 | INFO | ServerService Thread Pool -- 45 | org.jboss.as.server | WFLYSRV0010: Deployed "legacy.war" (runtime-name : "legacy.war")
    00:00:01,338 | INFO | MSC service thread 1-1 | org.jboss.weld.Version | WELD-000900: 5.1.2 (Final)
    00:00:01,342 | INFO | Controller Boot Thread | org.jboss.as.controller | WFLYCTL0183: Service status report
    +1
    00:00:01,410 | INFO | Controller Boot Thread | org.jboss.as.server | WFLYSRV0212: Resuming server
    00:00:01,413 | INFO | Controller Boot Thread | org.jboss.as | WFLYSRV0060: Http management interface listening on http://127.0.0.1:9990/management
    00:00:01,415 | ERROR | Controller Boot Thread | org.jboss.as | WFLYSRV0026: WildFly Full 33.0.0.Final (WildFly Core 25.0.0.Final) started (with errors) in 5313ms - Started 300 of 520 services (2 services failed or missing dependencies, 290 services are lazy, passive or on-demand) - Server configuration file in use: standalone.xml
    00:00:14,880 | INFO | default task-1 | stdout | Hello from (legacy) [servlet] - path /legacy : ok
    00:00:15,002 | DEBUG | default I/O-3 | io.undertow.request.io | UT005013: An IOException occurred
    00:00:20,117 | INFO | default task-1 | stdout |
    00:01:13,640 | WARN | Transaction Reaper | com.arjuna.ats.arjuna | ARJUNA012117: TransactionReaper::check timeout for TX 0:ffff7f000001:4b1a3f2e:66f0a1c2:2e in state  RUN
    00:02:00,000 | INFO | Thread-1 | org.jboss.as.server | WFLYSRV0220: Server shutdown has been requested via an OS signal
    00:02:00,145 | INFO | MSC service thread 1-2 | org.jboss.as | WFLYSRV0050: WildFly Full 33.0.0.Final (WildFly Core 25.0.0.Final) stopped in 120ms
    "#);
}

#[test]
fn every_fixture_has_at_least_20_entries() {
    for (preset, name) in fixtures() {
        let entries = stripped_lines(name)
            .iter()
            .filter(|line| parse(preset, line).is_some())
            .count();
        assert!(entries >= 20, "{name}: {entries} entries");
    }
}

/// Detection (#9) scores every preset on the same lines, so a line must start an entry
/// in its own preset or in none. There are no deliberate exceptions.
#[test]
fn no_preset_parses_a_line_of_another_presets_fixture() {
    let mut clashes = Vec::new();
    for (own, name) in fixtures() {
        for (n, line) in stripped_lines(name).iter().enumerate() {
            for (preset, _) in fixtures() {
                if preset != own && parse(preset, line).is_some() {
                    clashes.push(format!("{name}:{} parses as {preset:?}", n + 1));
                }
            }
        }
    }
    assert!(clashes.is_empty(), "{clashes:#?}");
}
