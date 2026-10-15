# Uses
- [constants](../constants.agent.md)
- [ServiceErrors](../records/service-errors.agent.md)

# Function: rank_services
Counts the error lines of every service and ranks the services, most errors first.

- in: error_lines: list of text
- out: list of ServiceErrors
- touches: nothing
- steps:
  1. group error_lines by regex.replace(item, log_pattern, "$4") into by_service
  2. empty list of ServiceErrors into ranking
  3. for each service, service_lines in by_service:
     1. add ServiceErrors to ranking:
        - service
        - errors: count of service_lines
  4. return sort ranking by errors descending
