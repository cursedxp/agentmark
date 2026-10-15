# Program: doubler
Twice a number, through a tool whose verb can fail.

# Tool: conv
A tool whose verb can fail: it is called with `try`, like a function that returns `result of`.

- touches: nothing

## double(word: text) -> result of int
1. try number.parse(word) into n
2. return n * 2

# Command: doubler
`doubler <word>` prints twice the number, or says it is not one.

- in: word: text
- steps:
  1. try conv double word into n
     otherwise:
     1. print: "not a number: {word}"
     2. stop
  2. print: "double {n}"
