# Tool: maths
Whole-number maths. Pure, and recursive: a verb may call itself.

- touches: nothing

## fact(n: int) -> int
1. if n <= 1:
   1. return 1
2. return n * maths.fact(n - 1)
