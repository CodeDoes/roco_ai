export const meta = {
  name: "fix-lsp-migration",
  description: "Migrate LSP CompletionRequest to new API",
  phases: ["read-source", "apply-fix", "verify"],
  pi: {
    tools: ["read", "write", "edit", "bash"]
  }
};

async function readAgentFile(path) {
  const data = await agent.read({ path, limit: 10000 });
  return data.text;
}

async function writeAgentFile(path, content) {
  await agent.write({ path, content });
}

async function checkCompile() {
  const result = await agent.bash({
    command: "cd /home/kit/Documents/dev/roco_ai && cargo check --workspace 2>&1",
    timeout: 300
  });
  const errorCount = (result.stdout.match(/error\[/g) || []).length;
  return { success: errorCount === 0, errorCount, output: result.stdout };
}

async function phase_read_source(agent, phase) {
  await phase.log("Reading source file: crates/cli/src/lsp.rs");
  const content = await readAgentFile("/home/kit/Documents/dev/roco_ai/crates/cli/src/lsp.rs");
  phase.log(`  File length: ${content.length} characters`);
  
  // Check for deprecated fields
  if (content.includes("system:")) {
    phase.log("  Found deprecated 'system' field in CompletionRequest");
  }
  if (content.includes("session:")) {
    phase.log("  Found deprecated 'session' field in CompletionRequest");
  }
  if (content.includes("preserve_state")) {
    phase.log("  Found deprecated 'preserve_state' field in CompletionRequest");
  }
  
  return { sourceContent: content };
}

async function phase_apply_fix(agent, phase, sourceData) {
  await phase.log("Applying fix to migrate LSP CompletionRequest to new API");
  
  const content = sourceData.sourceContent;
  
  // Replace the first (step1) CompletionRequest literal with the new pattern
  // Original:
  // let step1 = CompletionRequest {
  //     system: instruction.to_string(),
  //     prompt: bake_prompt,
  //     prefill: Some("<tool_call>
  // New:
  // let step1 = CompletionRequest {
  //     init_state: Some(FIM_SESSION.to_string()),
  //     state_slot: Some(FIM_SESSION.to_string()),
  //     prompt: format!("System: {}\n\n{}", instruction, FIM_FEW_SHOT),
  //     prefill: Some("<tool_call>
  
  const oldStep1Pattern = /(\s*let step1\s*=\s*CompletionRequest\s*\{)(\s*system\s*:\s*instruction\.to_string\(\),\s*\n\s*prompt\s*:\s*bake_prompt,\s*\n)/;
  
  // More targeted replacement using the exact text we found
  const step1Replacement = "let step1 = CompletionRequest {\n        // Use init_state/state_slot for session management instead of deprecated session field\n        init_state: Some(FIM_SESSION.to_string()),\n        state_slot: Some(FIM_SESSION.to_string()),\n        // Build the prompt with System/User/Assistant formatting\n        prompt: format!(\"System: {}\\n\\n\", instruction),\n        prefill: Some(\"";
  
  let newContent = content.replace(
    /(let step1\s*=\s*CompletionRequest\s*\{[\s\S]*?session: Some\(FIM_SESSION\.to_string\(\)\),\s*preserve_state: true,\s*\.\.Default::default\(\)\s*\})/,
    step1Replacement
  );
  
  // Also replace the second (step) CompletionRequest literal in the loop
  const stepReplacement = "        let step = CompletionRequest {\n        // Use init_state/state_slot for session management instead of deprecated session field\n        init_state: Some(FIM_SESSION.to_string()),\n        state_slot: Some(FIM_SESSION.to_string()),\n        // Build the prompt with System/User/Assistant formatting\n        prompt: format!(\"System: {}\\n\\n\", instruction),\n        prefill: Some(\"";
  
  newContent = newContent.replace(
    /(let step\s*=\s*CompletionRequest\s*\{[\s\S]*?session: Some\(FIM_SESSION\.to_string\(\)\),\s*preserve_state: true,\s*\.\.Default::default\(\)\s*\})/,
    stepReplacement
  );
  
  await writeAgentFile("/home/kit/Documents/dev/roco_ai/crates/cli/src/lsp.rs", newContent);
  phase.log("File updated successfully");
  
  return { fixedContent: newContent };
}

async function phase_verify(agent, phase, sourceData, fixData) {
  await phase.log("Verifying compilation after fix");
  const compile = await checkCompile();
  phase.log(`Compilation: ${compile.success ? '✅ Clean' : `❌ ${compile.errorCount} errors'`});
  
  if (compile.success) {
    phase.log("✅ Migration verified - no compilation errors");
  } else {
    phase.log("⚠️ Compilation errors found - may need additional fixes");
    // Show relevant error lines
    const errors = compile.output.match(/error\[[^\]]+\]/g) || [];
    errors.slice(0, 5).forEach(err => phase.log(`  ${err}`));
  }
  
  return compile;
}

export async function run(agent, args) {
  console.log("=== LSP Migration Workflow ===\n");
  
  const results = {};
  
  try {
    // Phase 1: Read source
    results.phase1 = await phase_read_source(agent, { log: (m) => console.log(m) });
    
    // Phase 2: Apply fix
    results.phase2 = await phase_apply_fix(agent, { log: (m) => console.log(m) }, results.phase1);
    
    // Phase 3: Verify
    results.phase3 = await phase_verify(agent, { log: (m) => console.log(m) }, results.phase1, results.phase2);
    
    console.log("\n=== Workflow Complete ===");
    console.log(`Final verification: ${results.phase3.success ? '✅ Clean' : '❌ Errors found'}`);
    
    return { results, success: results.phase3.success };
  } catch (e) {
    console.error(`Workflow failed: ${e.message}`);
    return { results: { error: e.message }, success: false };
  }
}