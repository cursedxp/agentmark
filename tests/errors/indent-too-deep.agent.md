> tests/errors/indent-too-deep.agent.md:12
>       - log: "nested too far"
>   ^^^^
>   This line is indented 4 spaces, but only 2 are possible here. Indent by one level (2 spaces) at a time.

# Agent: greeter
- model: claude
- tools: email, log

## On start
- log: "hello"
    - log: "nested too far"
