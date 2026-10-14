# Import: ../constants.agent.md

> The date in a camera file name written as 2026-10-03; empty text when the name is not a camera photo
> or when its date does not exist.
# Function: photo_stamp
- in: name: text
- out: text
- effect: pure
  - if regex.captures(name, camera_pattern) is some groups:
    - set digits = join groups with ""
    - try do date.parse(digits, "%Y%m%d") -> day
      otherwise:
        - return ""
    - return date.format(day, stamp_format)
  - return ""
