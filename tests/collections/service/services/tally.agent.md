# Uses
- [tally-state](../records/tally-state.agent.md)

# Service: tally
Counts names; the counts live in the service state and are served in key order.

- listen: 0.0.0.0:8080
- state: TallyState

## POST /count/{name}
1. if state.counts at name is some seen:
   1. put seen + 1 at name in state.counts
   else:
   1. put 1 at name in state.counts
2. add name to state.calls
3. respond 200 state.counts

## POST /forget/{name}
1. remove name from state.counts
2. respond 200 state.counts

## GET /summary
1. respond 200 { names: keys of state.counts, total: sum of values of state.counts, calls: count of state.calls }
