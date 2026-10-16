# S4: too much noise. Hide org.hibernate and org.jboss, then keep only
# com.mycompany.*.
Output docs/demos/s4-class-filter.gif
Source docs/demos/setup.tape

Type 'python3 "$DEMOS/demo-log.py" history server.log "2026-10-15 14:12" 40; clear'
Enter
Show

Type "jlv server.log"
Sleep 400ms
Enter
Sleep 2s
# The class panel, one rule per line.
Type "o"
Sleep 500ms
Type "!org.hibernate"
Down
Type "!org.jboss"
Sleep 500ms
Enter
Sleep 2s
Type "o"
Sleep 500ms
Type "com.mycompany.*"
Sleep 500ms
Enter
Sleep 2.5s
Type "q"
Sleep 300ms
