# 03 support-triage fixture

`stop` ends the whole trigger block (OPEN_QUESTIONS Q3), not just one loop iteration. T-2 already has a
priority, so the `Every 15 minutes` block ends at step 1.1 for T-2: T-3 and T-4 are never triaged and the trace has
no lines for them.
