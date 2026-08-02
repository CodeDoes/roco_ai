## 2026-08-02 - Tokenization Optimization

**Learning:** Reordering the split-map-filter sequence in string tokenization is extremely impactful. The original code mapped all split slices to lowercase strings before filtering out empty/short tokens, resulting in redundant heap allocations and copies for consecutive non-alphanumeric split characters. Putting the `.filter` step *before* `.map(|t| t.to_lowercase())` completely skips allocations for filtered tokens.

**Action:** Always filter string slices by length (or other constraints) prior to transforming them to owned strings/lowercasing them.
