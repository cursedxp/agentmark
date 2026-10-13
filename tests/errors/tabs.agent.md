> tests/errors/tabs.agent.md:12
>   	- log: "hello"
>   ^
>   Tabs are not allowed in AgentMark files.
>   Use 2 spaces for each indentation level.

# Agent: greeter
- model: claude
- tools: email, log

## On start
	- log: "hello"
