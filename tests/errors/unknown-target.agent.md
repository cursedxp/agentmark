> tests/errors/unknown-target.agent.md:11
>   - email send custmer: "Hello"
>                ^^^^^^^
>   Unknown target "custmer" for tool email. Did you mean "customer"?

# Agent: greeter
- model: claude
- tools: email, log

## On start
- email send custmer: "Hello"
