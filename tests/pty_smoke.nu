# pty parity smoke test - run via:
#   ./target/debug/http-nu eval -c "source tests/pty_smoke.nu"
# or
#   ./target/debug/http-nu eval tests/pty_smoke.nu
#
# Exercises pty open/write/resize/list/close end to end.

print "=== pty open bash ==="
let sid = (pty open bash --cols 80 --rows 24)
print $"opened sid=($sid)"

# Give bash time to print prompt
sleep 300ms

print "=== pty list ==="
let entries = (pty list)
print $"  count=($entries | length)"
print ($entries | first)

print "=== pty write ==="
"echo hello-from-wezterm-term\n" | pty write $sid
print "  wrote echo command"
sleep 300ms

print "=== pty resize ==="
pty resize $sid 120 30
print "  resized to 120x30"
sleep 100ms

print "=== pty meta set/get ==="
pty meta set $sid title "smoke test session"
let title = (pty meta get $sid title)
print $"  meta.title=($title)"

print "=== pty close ==="
pty close $sid
print "  closed"

print "=== final list ==="
print $"  remaining=(pty list | length)"

print "=== DONE ==="
