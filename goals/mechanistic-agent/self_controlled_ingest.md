# Self-Controlled Ingest

Intent: The controller decides what the model reads — context is pulled (not pushed) from memory, past sessions, workspace files, tools, and schemas, bounded by relevance windows the agent manages itself.

Sub-goals:
- Context window management: select what goes into the model's context each turn
- Relevance scoring: rank available context by relevance to the current intent
- Pull protocol: the controller fetches context on demand, rather than having it all prepended
