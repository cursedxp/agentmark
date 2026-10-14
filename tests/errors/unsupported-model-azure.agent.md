> tests/errors/unsupported-model-azure.agent.md:8
>   - model: azure:my-deployment
>            ^^^^^^^^^^^^^^^^^^^
>   The model provider "azure" is not supported in this version.
>   A model is claude, claude:<model>, openai:<model>, local:<model> or mock.

# Agent: greeter
- model: azure:my-deployment
- tools: log

## On start
- log: "hi"
