> tests/errors/unknown-tool.agent.md:13
>   1. emial send customer: "Hello"
>      ^^^^^
>   Unknown tool "emial". Did you mean "email"?

# Agent: greeter
Greets on start.

- model: claude
- tools: email, log

## On start
1. emial send customer: "Hello"
