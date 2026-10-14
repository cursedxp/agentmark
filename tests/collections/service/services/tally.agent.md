# Import: ../records/tally-state.agent.md

> Counts names; the counts live in the service state and are served in key order.
# Service: tally
- listen: 0.0.0.0:8080
- state: TallyState

## POST /count/{name}
- if state.counts at name is some seen:
  - put seen + 1 at name in state.counts
- else:
  - put 1 at name in state.counts
- append name to state.calls
- respond 200 state.counts

## POST /forget/{name}
- remove name from state.counts
- respond 200 state.counts

## GET /summary
- respond 200 { names: keys of state.counts, total: sum of values of state.counts, calls: count of state.calls }
