# Import: ../constants.agent.md

> The date in a camera file name written as 2026-10-03; none when the name is not a camera photo
> or when its date does not exist.
# Function: photo_stamp
- in: name: text
- out: maybe text
- effect: pure
  - if regex.captures(name, camera_pattern) is some groups:
    - set digits = join groups with ""
    - try date.parse(digits, digit_date_format) -> day
      otherwise:
        - return none
    - return some date.format(day, stamp_format)
  - return none
