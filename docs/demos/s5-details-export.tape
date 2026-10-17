# S5: found the error. Open its details, unfold the stack trace, copy it and
# export the filtered entries for a ticket.
Output docs/demos/s5-details-export.gif
Source docs/demos/setup.tape

# Copy over OSC 52, as over SSH, rather than into the clipboard of the machine
# that renders the tape.
Type 'export SSH_TTY=demo'
Enter
Type 'python3 "$DEMOS/demo-log.py" history server.log "2026-10-15 14:12" 40; clear'
Enter
Show

Type "vilog --level WARN,ERROR --since 10m server.log"
Sleep 400ms
Enter
Sleep 2s
# Back to the third exception from the end, unfold it, show its details.
Type "[[["
Sleep 1s
Type " "
Sleep 1.5s
Enter
Sleep 2s
Type "y"
Sleep 1.5s
# Export what the filters show; Enter takes the suggested name.
Type "e"
Sleep 1s
Enter
Sleep 2.5s
Type "q"
Sleep 300ms
