# Import: ../constants.agent.md
# Import: ../functions/rank-services.agent.md

> Reads an application log and prints how many errors, warnings and infos it holds, when the first
> error happened and which services fail most.
# Command: analyze-log
- args: path: text, top: int = 3

## Run
- try file read path -> content
  otherwise:
    - print: "Could not read {path}"
    - stop
- set lines = text.lines(content)
- set entries = lines where regex.matches(item, log_pattern)
- set error_lines = entries where regex.matches(item, error_prefix)
- set warning_count = count of entries where regex.matches(item, warn_prefix)
- set info_count = count of entries where regex.matches(item, info_prefix)
- print: "Log report for {path}"
- print: "Lines read: {count of lines}"
- print: "Entries: {count of entries}"
- print: "Errors: {count of error_lines}  Warnings: {warning_count}  Info: {info_count}"
- if first of error_lines is some earliest:
  - if regex.captures(earliest, stamp_pattern) is some parts:
    - set moment = join parts with ""
    - print: "First error at {moment}"
- set ranked = rank_services(error_lines)
- set busiest = take top from ranked
- print: "Top {top} services by errors:"
- for each entry in busiest:
  - print: "  {text.pad(entry.service, 10)}{entry.errors}"
