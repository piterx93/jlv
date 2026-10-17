#!/usr/bin/env python3
"""Writes the WildFly server.log the demo tapes open.

    demo-log.py history FILE END MINUTES   entries for the MINUTES before END
                                           ("YYYY-MM-DD HH:MM" or "now"), with
                                           a burst of payment errors at 14:05
    demo-log.py live FILE SECONDS          appends a deploy and traffic, stamped
                                           with the wall clock, for SECONDS

With a fixed END, history writes the same file on every run, so the GIFs only
change when the tapes do.
"""

import random
import sys
import time
from datetime import datetime, timedelta

PAY_TRACE = """java.net.SocketTimeoutException: Read timed out
\tat java.base/sun.nio.ch.NioSocketImpl.timedRead(NioSocketImpl.java:278)
\tat java.base/sun.nio.ch.NioSocketImpl.implRead(NioSocketImpl.java:304)
\tat java.base/java.net.Socket$SocketInputStream.read(Socket.java:1099)
\tat com.mycompany.shop.pay.HttpTransport.send(HttpTransport.java:132)
\tat com.mycompany.shop.pay.PayClient.post(PayClient.java:88)
\tat com.mycompany.shop.pay.PayClient.charge(PayClient.java:41)
\tat com.mycompany.shop.order.OrderService.checkout(OrderService.java:203)
\tat com.mycompany.shop.api.CheckoutResource.post(CheckoutResource.java:57)
Caused by: java.io.IOException: gateway.pay.example: connection reset
\tat com.mycompany.shop.pay.HttpTransport.read(HttpTransport.java:171)
\t... 14 more"""

NOT_FOUND_TRACE = """jakarta.ws.rs.NotFoundException: HTTP 404 Not Found
\tat org.jboss.resteasy.core.registry.SegmentNode.match(SegmentNode.java:117)
\tat org.jboss.resteasy.core.registry.RootNode.match(RootNode.java:43)
\t... 58 more"""

POOL_TRACE = """org.hibernate.exception.JDBCConnectionException: Unable to acquire JDBC Connection
\tat org.hibernate.exception.internal.SQLStateConversionDelegate.convert(SQLStateConversionDelegate.java:100)
\tat org.hibernate.engine.jdbc.connections.internal.LogicalConnectionManagedImpl.acquireConnectionIfNeeded(LogicalConnectionManagedImpl.java:113)
\tat com.mycompany.shop.order.OrderRepository.findOpen(OrderRepository.java:64)
Caused by: java.sql.SQLException: IJ031084: Unable to create connection
\t... 31 more"""

DEPLOY = [
    ("INFO", "org.jboss.as.server.deployment", "MSC service thread 1-3",
     'WFLYSRV0027: Starting deployment of "shop.war" (runtime-name: "shop.war")'),
    ("INFO", "org.jboss.weld.deployer", "MSC service thread 1-1",
     "WFLYWELD0003: Processing weld deployment shop.war"),
    ("INFO", "org.hibernate.jpa.internal.util.LogHelper", "ServerService Thread Pool -- 78",
     "HHH000204: Processing PersistenceUnitInfo [name: shopPU]"),
    ("WARN", "org.hibernate.orm.deprecation", "ServerService Thread Pool -- 78",
     "HHH90000025: H2Dialect does not need to be specified explicitly"),
    ("INFO", "org.wildfly.extension.undertow", "ServerService Thread Pool -- 78",
     "WFLYUT0021: Registered web context: '/shop' for server 'default-server'"),
    ("INFO", "org.jboss.as.server", "ServerService Thread Pool -- 45",
     'WFLYSRV0010: Deployed "shop.war" (runtime-name : "shop.war")'),
]


def entry(rng, incident):
    """One entry as (level, class, thread, message); errors are likelier during an
    incident."""
    task = f"default task-{rng.randint(1, 12)}"
    roll = rng.random()
    if roll < (0.25 if incident else 0.015):
        order = rng.randint(4000, 9999)
        return ("ERROR", "com.mycompany.shop.pay.PayClient", task,
                f"Payment {order} failed\n{PAY_TRACE}")
    if roll < (0.35 if incident else 0.04):
        attempt = rng.randint(1, 3)
        return ("WARN", "com.mycompany.shop.pay.PayClient", task,
                f"Retry {attempt}/3 after read timeout (gateway.pay.example)")
    if roll < 0.055:
        return ("ERROR", "io.undertow.request", task,
                f"UT005023: Exception handling request to /shop/api/cart\n{NOT_FOUND_TRACE}")
    if roll < 0.06:
        return ("ERROR", "org.hibernate.engine.jdbc.spi.SqlExceptionHelper", task,
                f"IJ031084: Unable to create connection\n{POOL_TRACE}")
    if roll < 0.09:
        return ("WARN", "org.hibernate.engine.jdbc.spi.SqlExceptionHelper", task,
                "SQL Warning Code: 0, SQLState: 01000")
    if roll < 0.30:
        customer = rng.randint(100, 999)
        return ("DEBUG", "org.hibernate.SQL", task,
                f"select o1_0.id,o1_0.status,o1_0.total from orders o1_0 where o1_0.customer_id={customer}")
    if roll < 0.40:
        return ("DEBUG", "com.mycompany.shop.order.OrderRepository", task,
                f"Loaded {rng.randint(1, 60)} rows in {rng.randint(2, 90)} ms")
    if roll < 0.48:
        return ("INFO", "org.jboss.as.ejb3", task,
                "WFLYEJB0473: JNDI bindings for session bean named 'CartBean' in deployment unit 'shop.war'")
    if roll < 0.70:
        return ("INFO", "com.mycompany.shop.pay.PayClient", task,
                f"POST /payments -> 200 in {rng.randint(40, 400)} ms")
    order = rng.randint(4000, 9999)
    return ("INFO", "com.mycompany.shop.order.OrderService", task,
            f"Order {order} created for customer {rng.randint(100, 999)}")


def line(at, level, logger, thread, message):
    stamp = at.strftime("%Y-%m-%d %H:%M:%S,") + f"{at.microsecond // 1000:03d}"
    return f"{stamp} {level:<5} [{logger}] ({thread}) {message}\n"


def history(path, end, minutes):
    end = datetime.now() if end == "now" else datetime.strptime(end, "%Y-%m-%d %H:%M")
    start = end - timedelta(minutes=int(minutes))
    rng = random.Random(f"{end:%H:%M}/{minutes}")
    # The incident: the payment gateway times out from 14:05 to 14:08.
    incident_start = end.replace(hour=14, minute=5, second=0, microsecond=0)
    incident = (incident_start, incident_start + timedelta(minutes=3))
    at = start
    with open(path, "w") as out:
        while at < end:
            busy = incident[0] <= at < incident[1]
            out.write(line(at, *entry(rng, busy)))
            at += timedelta(milliseconds=rng.randint(200, 2600))


def live(path, seconds):
    rng = random.Random(7)
    stop = time.monotonic() + float(seconds)
    time.sleep(1.5)
    for step in DEPLOY:
        append(path, step)
        time.sleep(0.6)
    while time.monotonic() < stop:
        append(path, entry(rng, False))
        time.sleep(rng.uniform(0.4, 1.2))


def append(path, fields):
    with open(path, "a") as out:
        out.write(line(datetime.now(), *fields))


if __name__ == "__main__":
    commands = {"history": (history, 3), "live": (live, 2)}
    if len(sys.argv) < 2 or sys.argv[1] not in commands:
        sys.exit(__doc__)
    command, arity = commands[sys.argv[1]]
    if len(sys.argv) != 2 + arity:
        sys.exit(__doc__)
    command(*sys.argv[2:])
