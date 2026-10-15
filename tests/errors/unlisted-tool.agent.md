> tests/errors/unlisted-tool.agent.md:14
>   1. crm update invoice: status = "reminded"
>      ^^^
>   The tool "crm" is not listed in the tools of agent "greeter".
>   Add it to the "- tools:" line, for example "- tools: email, log, crm".

# Agent: greeter
Greets on start.

- model: claude
- tools: email, log

## On start
1. crm update invoice: status = "reminded"
