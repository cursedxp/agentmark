> tests/errors/unknown-verb.agent.md:11
>   - email sned customer: "Hello"
>           ^^^^
>   Unknown verb "sned" for tool email. Did you mean "send"?

# Agent: greeter
- model: claude
- tools: email, log

## On start
- email sned customer: "Hello"
