> tests/errors/bad-indentation.agent.md:12
>      - log: "hello"
>   ^^^
>   Indentation must be 2 spaces per level, but this line is indented 3 spaces.
>   Use 2 or 4 spaces.

# Agent: greeter
- model: claude
- tools: email, log

## On start
   - log: "hello"
