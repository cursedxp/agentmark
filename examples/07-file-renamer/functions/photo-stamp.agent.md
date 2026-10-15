# Uses
- [constants](../constants.agent.md)

# Function: photo_stamp
The date in a camera file name written as 2026-10-03; none when the name is not a camera photo
or when its date does not exist.

- in: name: text
- out: maybe text
- touches: nothing
- steps:
  1. if regex.captures(name, camera_pattern) is some groups:
     1. join groups with "" into digits
     2. try date.parse(digits, digit_date_format) into day
        otherwise:
        1. return none
     3. return some date.format(day, stamp_format)
  2. return none
