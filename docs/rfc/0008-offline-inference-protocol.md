# RFC 0008: Offline Inference Protocol for Remote Locations
Status: Required for Edge Deployment
Model path must be absolute local file (no URLs). MockBackend.generate is temporary placeholder; production replaces with roco-inferd RWKV backend calling local .st model. No remote backend fallback permitted in secure/off-grid mode. Gateway module disabled when strict_grammar = true.
