FULL STACK STATUS (mocked):
- framework.rs: DomainHarness trait + MockBackend + Context/State/HarnessError (REAL)
- full_stack.rs: integrated loop with retry/rollback/verify/history tracking (REAL)
- 11 domain modules (writing, coding, html, chat, org, pet, debug, email, research, aggregate, browser): REAL implementations using MockBackend (REAL code bodies)
- use_cases/all_70.rs: 14 categories mapping all 70 use cases (REAL constants)
- USE_CASES_70_MAPPED.md: mapping doc (REAL)
- local_agent/mod.rs: exports all (REAL)
- Framework clones (test_clones/): copied files (REAL files)
- evaluation scripts (evals/*.sh): echo scripts (REAL files)
- Previous crate tests: basic smoke tests (REAL files)
- framework_diagram.png + explanation.mp3 (REAL media)

NOT REAL:
- No RWKV inference connection
- No actual file workspace enforcement
- No real text generation from model weights
- No validation engine running actual checks
- MockBackend returns formatted strings only
