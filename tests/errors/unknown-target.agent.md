> tests/errors/unknown-target.agent.md:13
>   1. email send custmer: "Hello"
>                 ^^^^^^^
>   Unknown target "custmer" for tool email. Did you mean "customer"?

# Agent: greeter
Greets on start.

- model: claude
- tools: email, log

## On start
1. email send custmer: "Hello"
