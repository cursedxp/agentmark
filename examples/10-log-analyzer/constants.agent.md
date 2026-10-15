# Constant: log_pattern
One log line: date, time, level, service and message, separated by single spaces.

- value: "^([0-9-]+) ([0-9:]+) ([A-Z]+) ([a-z]+) (.*)$"

# Constant: error_prefix
The start of a log line up to and including the level, so " ERROR " in a message is not counted.

- value: "^[^ ]+ [^ ]+ ERROR "

# Constant: warn_prefix
- value: "^[^ ]+ [^ ]+ WARN "

# Constant: info_prefix
- value: "^[^ ]+ [^ ]+ INFO "

# Constant: stamp_pattern
Date and time at the start of a line, caught as one piece.

- value: "^([0-9-]+ [0-9:]+) "
