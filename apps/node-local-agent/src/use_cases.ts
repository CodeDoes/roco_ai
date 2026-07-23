/**
 * All 70 local AI use cases mapped to framework modules.
 * Each category contains its count and a description of its mapping.
 */

export interface UseCaseCategory {
  count: number;
  mapping: string;
  description: string;
}

export const USE_CASES: Record<string, UseCaseCategory> = {
  privacy_security: {
    count: 7,
    mapping: 'workspace + session + validation',
    description: 'redaction, secure docs',
  },
  offline_edge: {
    count: 5,
    mapping: 'inference (local RWKV) + gateway (local server)',
    description: 'on-device models, offline operation',
  },
  cost_infrastructure: {
    count: 4,
    mapping: 'framework (model selection) + harness config',
    description: 'infrastructure cost controls, adaptive limits',
  },
  personalization_fine_tuning: {
    count: 5,
    mapping: 'agent-core (memory, fine-tuning hooks) + grammar (DSL)',
    description: 'custom formatting rules, behavior tuning',
  },
  creative_arts: {
    count: 6,
    mapping: 'agent-story (narrative, image prompts) + inference',
    description: 'story generation, world wiki generation',
  },
  coding: {
    count: 5,
    mapping: 'agent-core (mecha_agent, tool_selector) + workspace',
    description: 'code generation, unit test creation, refactoring',
  },
  productivity_knowledge: {
    count: 5,
    mapping: 'session (RAG) + message (formatting) + grammar (structured output)',
    description: 'document Q&A, email templates, summarization',
  },
  home_automation: {
    count: 4,
    mapping: 'ui (widgets) + chat-common + server (local routes)',
    description: 'smart home routing, local alerts',
  },
  gaming: {
    count: 4,
    mapping: 'agent-story (procedural dialogue, lore) + engine (eval pipeline)',
    description: 'procedural quest lore, NPC dialog validation',
  },
  education: {
    count: 4,
    mapping: 'agent-core (context) + message (prompt formatting) + session',
    description: 'clinical cases simulation, tutor loops',
  },
  data_analysis: {
    count: 4,
    mapping: 'engine (eval framework) + workspace (CSV/file tools)',
    description: 'log analysis, database report aggregation',
  },
  accessibility: {
    count: 4,
    mapping: 'ui + message + chat-common (STT/TTS interfaces)',
    description: 'voice assistant integration, read-out-loud widgets',
  },
  research_tinkering: {
    count: 5,
    mapping: 'engine + inference + framework (experiment loops)',
    description: 'multi-turn feedback research, validation testing',
  },
  niche_edge: {
    count: 8,
    mapping: 'full_stack (mock integration of all above layers)',
    description: 'integrated offline automation loops',
  },
};

export const TOTAL_USE_CASES_COUNT = Object.values(USE_CASES).reduce(
  (sum, category) => sum + category.count,
  0
);
