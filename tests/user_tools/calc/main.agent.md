# Import: ./tools as tools

> The same factorial as a function and as a tool verb. Every argument has a default, so `calc` alone runs.
# Command: calc
- args: n: int = 5, label: text = "n"

## Run
- tools.printer say "{label}! = {fact(n)}"
- tools.printer say "{tools.maths.fact(n)}"

> Factorial, calling itself.
# Function: fact
- in: n: int
- out: int
- effect: pure
  - if n <= 1:
    - return 1
  - return n * fact(n - 1)

# Run
- calc
