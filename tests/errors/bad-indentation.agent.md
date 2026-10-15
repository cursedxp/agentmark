> tests/errors/bad-indentation.agent.md:14
>       2. print: "bye"
>   ^^^^
>   This line is indented 4 spaces, less than the 5 of the lines above it at this level.
>   Lines of one list start at the same column.

# Command: greet
Says hello to each name twice.

- in: names: list of text
- steps:
  1. for each name in names:
     1. print: "hello"
    2. print: "bye"
