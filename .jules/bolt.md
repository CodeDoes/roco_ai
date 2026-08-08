## 2026-08-02 - Tokenization Optimization

**Learning:** Reordering the split-map-filter sequence in string tokenization is extremely impactful. The original code mapped all split slices to lowercase strings before filtering out empty/short tokens, resulting in redundant heap allocations and copies for consecutive non-alphanumeric split characters. Putting the `.filter` step *before* `.map(|t| t.to_lowercase())` completely skips allocations for filtered tokens.

**Action:** Always filter string slices by length (or other constraints) prior to transforming them to owned strings/lowercasing them.

## 2026-08-03 - Static OnceLock Cache for Word Lists

**Learning:** Initializing static or default lists (such as common spelling words or dictionaries) on struct defaults can silently lead to massive allocation storms. Every instance of `ChapterValidator::default()` was allocating over 500 owned String instances inside a `HashSet`. Storing string slices (`HashSet<&'static str>`) instead of `HashSet<String>` and caching the set globally with `std::sync::OnceLock` completely eliminates heap allocations on subsequent instantiations.

**Action:** Use `OnceLock` with `&'static str` for reference/dictionary lookups to keep struct defaults and instantiations allocation-free.
