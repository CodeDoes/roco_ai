## 2026-08-02 - Tokenization Optimization

**Learning:** Reordering the split-map-filter sequence in string tokenization is extremely impactful. The original code mapped all split slices to lowercase strings before filtering out empty/short tokens, resulting in redundant heap allocations and copies for consecutive non-alphanumeric split characters. Putting the `.filter` step *before* `.map(|t| t.to_lowercase())` completely skips allocations for filtered tokens.

**Action:** Always filter string slices by length (or other constraints) prior to transforming them to owned strings/lowercasing them.

## 2026-10-24 - Static Collection Caching with OnceLock

**Learning:** Initializing large collections of static string literals (such as word lists for validation or processing) using `HashSet<String>` dynamically inside struct initializers causes massive overhead from hundreds of heap allocations and string conversions on every instantiation. Utilizing `HashSet<&'static str>` instead of `HashSet<String>` eliminates individual string allocations completely, and using `std::sync::OnceLock` ensures the `HashSet` is constructed exactly once.

**Action:** Use `HashSet<&'static str>` and `OnceLock` for static read-only collections to keep struct instantiation free of dynamic string allocations.
