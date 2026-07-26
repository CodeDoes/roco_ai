# RFC 0005: Multi-Agent Coordination Protocol
Status: Speculative / Experimental

## Inter-Agent Protocol Specs
- Meta-Agent (`aggregate` domain) receives request and routes to target `DomainHarness`.
- State sharing occurs via immutable `Context` snapshot passing.
- Cascading Rollback: If sub-agent fails verification after `max_attempts`, meta-agent rolls back selection state and re-routes request to secondary agent.
- Sandbox Scoping: Each sub-agent gets a distinct `Sandbox` instance with scoped directory paths.
