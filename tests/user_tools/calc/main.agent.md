# Program: calc
Factorial, once as a function and once as a tool verb.

# Uses
- [tools](./tools/) as tools

# Command: calc
The same factorial as a function and as a tool verb. Every argument has a default, so `calc` alone runs.

- in: n: int = 5, label: text = "n"
- steps:
  1. tools.printer say "{label}! = {fact(n)}"
  2. tools.printer say "{tools.maths.fact(n)}"

# Function: fact
Factorial, calling itself.

- in: n: int
- out: int
- touches: nothing
- steps:
  1. if n <= 1:
     1. return 1
  2. return n * fact(n - 1)
