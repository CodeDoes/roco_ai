export const meta = {
  name: "migrate-agents-audit",
  description: "Audit agent validation modules for deprecated fields",
  phases: ["verify-start", "audit-files", "report"],
  pi: { tools: ["read", "bash"] }
};

async function readAgentFile(path) {
  const data = await agent.read({ path, limit: 30000 });
  return data.text;
}

async function checkCompile() {
  const result = await agent.bash({
    command: "cd /home/kit/Documents/dev/roco_ai && cargo check --workspace 2>&1",
    timeout: 300
  });
  const errorCount = (result.stdout.match(/error\[/g) || []).length;
  return { success: errorCount === 0, errorCount };
}

function hasDeprecatedFields(content) {
  return content.includes("system:") || content.includes(".session(") || content.includes("preserve_state");
}

async function phase_verify_start(agent, phase) {
  await phase.log("Phase 1: Verify compilation baseline");
  const compile = await checkCompile();
  await phase.log(`  Compilation: ${compile.success ? '✅ Clean' : `⚠️ ${compile.errorCount} errors`}`);
  return compile;
}

async function phase_audit_files(agent, phase) {
  await phase.log("Phase 2: Audit agent validation files for deprecated fields");
  
  const validationFiles = [
    { path: "/home/kit/Documents/dev/roco_ai/crates/agent/src/validation/intent.rs", name: "intent.rs" },
    { path: "/home/kit/Documents/dev/roco_ai/crates/agent/src/validation/planner.rs", name: "planner.rs" },
    { path: "/home/kit/Documents/dev/roco_ai/crates/agent/src/validation/summarizer.rs", name: "summarizer.rs" },
    { path: "/home/kit/Documents/dev/roco_ai/crates/agent/src/validation/wiki.rs", name: "wiki.rs" },
    { path: "/home/kit/Documents/dev/roco_ai/crates/agent/src/validation/brainstorm.rs", name: "brainstorm.rs" },
    { path: "/home/kit/Documents/dev/roco_ai/crates/agent/src/validation/inference.rs", name: "inference.rs" },
    { path: "/home/kit/Documents/dev/roco_ai/crates/agent/src/validation/agent.rs", name: "agent.rs" }
  ];
  
  const results = [];
  for (const f of validationFiles) {
    try {
      const content = await readAgentFile(f.path);
      const hasDep = hasDeprecatedFields(content);
      const simpleName = f.name;
      if (hasDep) {
        await phase.log(`  ⚠️ ${simpleName} - REQUIRES MIGRATION`);
      } else {
        await phase.log(`  ✅ ${simpleName} - already clean`);
      }
      results.push({ file: simpleName, requiresMigration: hasDep });
    } catch (e) {
      await phase.log(`  ❌ Error reading ${f.path}: ${e.message}`);
      results.push({ file: f.name, error: e.message });
    }
  }
  return { auditResults: results };
}

async function phase_report(agent, phase) {
  await phase.log("Phase 3: Generate migration plan");
  await phase.log(
    `Based on audit findings:\n` +
    `- Files requiring manual migration will be fixed one-by-one\n` +
    `- Each fix will be verified with cargo check after\n` +
    `- The conservative one-at-a-time approach prevents cascading errors\n` +
    `- Temporary deprecation stubs remain in types.rs until full migration complete\n` +
    `\nNext step: Begin with intent.rs using targeted edits with proper anchors.`
  );
  return { status: "plan_ready" };
}

export async function run(agent, args) {
  console.log("=== Agent Validation Module Audit ===\n");
  const results = {};
  try {
    results.phase1 = await phase_verify_start(agent, { log: (m) => console.log(m) });
    results.phase2 = await phase_audit_files(agent, { log: (m) => console.log(m) });
    results.phase3 = await phase_report(agent, { log: (m) => console.log(m) });
    console.log("\n=== Audit Complete ===");
    console.log(JSON.stringify(results, null, 2));
    return { results, success: true };
  } catch (e) {
    console.error(`Workflow failed: ${e.message}`);
    return { results: { error: e.message }, success: false };
  }
}
