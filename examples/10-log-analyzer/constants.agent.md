> One log line: date, time, level, service and message, separated by single spaces.
# Constant: log_pattern
- value: "^([0-9-]+) ([0-9:]+) ([A-Z]+) ([a-z]+) (.*)$"

> The start of a log line up to and including the level, so " ERROR " in a message is not counted.
# Constant: error_prefix
- value: "^[^ ]+ [^ ]+ ERROR "

# Constant: warn_prefix
- value: "^[^ ]+ [^ ]+ WARN "

# Constant: info_prefix
- value: "^[^ ]+ [^ ]+ INFO "

> Date and time at the start of a line, caught as one piece.
# Constant: stamp_pattern
- value: "^([0-9-]+ [0-9:]+) "
