> tests/errors/unknown-tool.agent.md:11
>   - emial send customer: "Hello"
>     ^^^^^
>   Unknown tool "emial". Did you mean "email"?

# Agent: greeter
- model: claude
- tools: email, log

## On start
- emial send customer: "Hello"
