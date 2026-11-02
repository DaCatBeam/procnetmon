# Records docs/demo.gif with simulated traffic. Run it with `make record-demo`, which builds the
# app and examples/demo_traffic.rs first. It needs VHS: https://github.com/charmbracelet/vhs
#
# The waits are timed to the generator's events, counted from when it starts: a container starts
# at 8 s, another stops at 10.5 s, and eth0 downloads from 22 s to 29 s. Change them together.

Output docs/demo.gif

Set Shell "bash"
Set FontSize 18
Set Width 1200
Set Height 680
Set Padding 20
Set Framerate 24
Set TypingSpeed 50ms
Set WindowBar Colorful

# Start the simulated traffic out of sight. It runs until the end.
Hide
Type@10ms 'export PATH="$PWD/target/release:$PATH"'
Enter
Type@10ms 'target/release/examples/demo_traffic /tmp/procnetmon-demo.txt 2>/dev/null &'
Enter
Type@10ms "clear"
Enter
Sleep 500ms
Show

# The app starts about 4 s in.
Type "procnetmon -p 250 -w 20 --file /tmp/procnetmon-demo.txt"
Sleep 500ms
Enter
Sleep 1s

# The overview, while one container starts and another stops.
Tab
Sleep 300ms
Down
Sleep 6500ms

# Session totals, then errors and drops.
Down
Sleep 2500ms
Down
Sleep 4s

# Back to the throughput chart, then over to eth0 in time for its download.
Up 3
Sleep 400ms
Left
Sleep 300ms
Down
Sleep 10s

Type "q"
Sleep 1500ms

Hide
Type "kill %1"
Enter
