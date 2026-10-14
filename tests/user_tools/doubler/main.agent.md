> A tool whose verb can fail: it is called with `try`, like a function that returns `result of`.
# Tool: conv
- effect: pure

## double(word: text) -> result of int
- try number.parse(word) -> n
- return n * 2

> `doubler <word>` prints twice the number, or says it is not one.
# Command: doubler
- args: word: text

## Run
- try conv double word -> n
  otherwise:
    - print: "not a number: {word}"
    - stop
- print: "double {n}"

# Run
- doubler
