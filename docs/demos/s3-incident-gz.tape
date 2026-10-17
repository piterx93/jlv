# S3: an incident at 14:05 yesterday. Open the rotated .gz, keep 14:00-14:15 and
# jump from one exception to the next.
Output docs/demos/s3-incident-gz.gif
Source docs/demos/setup.tape

Type 'python3 "$DEMOS/demo-log.py" history server.log.2026-10-14 "2026-10-14 16:00" 180'
Enter
Type 'gzip server.log.2026-10-14; clear'
Enter
Show

Type "vilog server.log.2026-10-14.gz"
Sleep 400ms
Enter
Sleep 2s
Type "s"
Sleep 500ms
Type "14:00-14:15"
Sleep 500ms
Enter
Sleep 1.5s
# Jump to 14:05, then from exception to exception.
Type "t"
Sleep 300ms
Type "14:05"
Sleep 300ms
Enter
Sleep 1s
Type "]"
Sleep 1s
Type "]"
Sleep 1s
Type "]"
Sleep 1s
Enter
Sleep 2.5s
Type "q"
Sleep 300ms
