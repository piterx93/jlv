# S2: what happened in the last 10 minutes? Narrow to them, then hide TRACE, DEBUG
# and INFO so only WARN and up are left.
Output docs/demos/s2-last-10-minutes.gif
Source docs/demos/setup.tape

Type 'python3 "$DEMOS/demo-log.py" history server.log "2026-10-15 14:12" 40; clear'
Enter
Show

Type "vilog server.log"
Sleep 400ms
Enter
Sleep 2s
# The time panel; Down steps through the presets 1m 5m 10m.
Type "s"
Sleep 800ms
Down 3
Sleep 800ms
Enter
Sleep 1.5s
Type "1"
Sleep 300ms
Type "2"
Sleep 300ms
Type "3"
Sleep 3s
Type "q"
Sleep 300ms
