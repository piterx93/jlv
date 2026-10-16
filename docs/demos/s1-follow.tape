# S1: watch live traffic during a deploy. A background writer appends to the log
# while jlv follows it.
Output docs/demos/s1-follow.gif
Source docs/demos/setup.tape

Type 'python3 "$DEMOS/demo-log.py" history server.log now 30'
Enter
Type '(python3 "$DEMOS/demo-log.py" live server.log 13 &)'
Enter
Type 'clear'
Enter
Show

Type "jlv server.log"
Sleep 400ms
Enter
Sleep 5s
# Moving up pauses following and counts what arrives meanwhile.
Type "kkkkk"
Sleep 3s
Type "G"
Sleep 3s
Type "q"
Sleep 500ms
