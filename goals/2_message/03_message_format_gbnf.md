# Message Format (GBNF)

Intent: Constrain chat messages to a structured schema via GBNF so message boundaries and roles are machine-parseable.


## Example 1
```
System:...

User:...

Assistant:...
```

## Example 2
```
System:...

User:...

Assistant:<think>...</think>...
```

## Example 3
```
System:...<tools>{...}</tools>

User:...

Assistant:<think>...</think><tool_call>{...}</tool_call><tool_result>{...}</tool_result>...
```

## Example 4
```
System:...<tools>{...}</tools>

User:...

Assistant:<tool_call>{...}</tool_call><tool_result>{...}</tool_result>...
User:...?

Assistant:<think>...</think>...!
```
