# Render from the repo root:
#   cargo build --release --target x86_64-unknown-linux-musl
#   docker run --rm -v "$PWD:/vhs" ghcr.io/charmbracelet/vhs docs/demo.tape
Output docs/assets/demo.gif

Set Shell bash
Set FontSize 18
Set Width 1200
Set Height 620
Set Padding 24
Set Theme "Catppuccin Mocha"
Set TypingSpeed 35ms

Hide
Type "cd /vhs/docs/demo && export PATH=/vhs/target/x86_64-unknown-linux-musl/release:$PATH PS1='$ ' && clear"
Enter
Sleep 0.5s
Show

Type "cssdiff main refactor"
Enter
Sleep 2s

Type "cssdiff main broken"
Enter
Sleep 6s
