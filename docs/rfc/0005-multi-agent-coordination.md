# RFC 0005: Multi-Agent Coordination Protocol
Status: Speculative / Experimental
Defines inter-agent message passing via Context objects. Each agent exposes DomainHarness. A meta-agent (aggregate domain) selects sub-agents based on input classification. Rollback cascades: if sub-agent fails, meta-agent rolls back its selection state and retries with alternate agent. Sandbox isolation ensures tool access is scoped per agent.
