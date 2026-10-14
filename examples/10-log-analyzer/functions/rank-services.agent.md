# Import: ../constants.agent.md
# Import: ../records/service-errors.agent.md

> Counts the error lines of every service and ranks the services, most errors first.
# Function: rank_services
- in: error_lines: list of text
- out: list of ServiceErrors
- effect: pure
  - set by_service = group error_lines by regex.replace(item, log_pattern, "$4")
  - set ranking = empty list of ServiceErrors
  - for each service, rows in by_service:
    - append ServiceErrors { service: service, errors: count of rows } to ranking
  - return sort ranking by errors descending
