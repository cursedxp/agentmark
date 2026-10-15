# Uses
- [constants](../constants.agent.md)
- [rank_services](../functions/rank-services.agent.md)

# Command: analyze-log
Reads an application log and prints how many errors, warnings and infos it holds, when the first
error happened and which services fail most.

- in: path: text, top: int = 3
- steps:
  1. try file read path into content
     otherwise:
     1. print: "Could not read {path}"
     2. stop
  2. text.lines(content) into lines
  3. lines where regex.matches(item, log_pattern) into entries
  4. entries where regex.matches(item, error_prefix) into error_lines
  5. count of entries where regex.matches(item, warn_prefix) into warning_count
  6. count of entries where regex.matches(item, info_prefix) into info_count
  7. print: "Log report for {path}"
  8. print: "Lines read: {count of lines}"
  9. print: "Entries: {count of entries}"
  10. print: "Errors: {count of error_lines}  Warnings: {warning_count}  Info: {info_count}"
  11. if first of error_lines is some earliest:
      1. if regex.captures(earliest, stamp_pattern) is some parts:
         1. join parts with "" into moment
         2. print: "First error at {moment}"
  12. rank_services(error_lines) into ranked
  13. take top from ranked into busiest
  14. print: "Top {top} services by errors:"
  15. for each entry in busiest:
      1. print: "  {text.pad(entry.service, 10)}{entry.errors}"
