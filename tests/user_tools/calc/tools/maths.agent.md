> Whole-number maths. Pure, and recursive: a verb may call itself.
# Tool: maths
- effect: pure

## fact(n: int) -> int
- if n <= 1:
  - return 1
- return n * maths.fact(n - 1)
