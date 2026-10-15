> tests/errors/unsupported-model-azure.agent.md:10
>   - model: azure:my-deployment
>            ^^^^^^^^^^^^^^^^^^^
>   The model provider "azure" is not supported in this version.
>   A model is claude, claude:<model>, openai:<model>, local:<model> or mock.

# Agent: greeter
Greets on start.

- model: azure:my-deployment
- tools: log

## On start
1. log: "hi"
