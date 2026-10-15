> tests/errors/tabs.agent.md:14
>   	1. log: "hello"
>   ^
>   Tabs are not allowed in AgentMark files.
>   Indent with spaces: 3 under a numbered step, 2 under a "- " line.

# Agent: greeter
Greets on start.

- model: claude
- tools: email, log

## On start
	1. log: "hello"
