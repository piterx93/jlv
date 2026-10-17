# Shared by the scenario tapes. Each one is run from the repository root after
# `cargo build --release`, e.g. `vhs docs/demos/s1-follow.tape`.

Set Shell bash
Set Columns 100
Set Rows 30
Set FontSize 12
Set Padding 12
Set Framerate 8
Set TypingSpeed 60ms
Set Theme "Builtin Dark"

Hide
Type 'export PATH="$PWD/target/release:$PATH" DEMOS="$PWD/docs/demos" XDG_CONFIG_HOME="$PWD/docs/demos"'
Enter
Type 'cd "$(mktemp -d)"'
Enter
