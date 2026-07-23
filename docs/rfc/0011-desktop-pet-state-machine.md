# RFC 0011: Desktop Pet State Machine
Status: Experimental
States: SLEEP, AWAKE, CURIOUS, BORED, EXCITED. Transitions triggered by user mouse movement frequency and session duration. Pet runs as persistent ui widget using roco_ui crate. Inference called only when pet is AWAKE (resource conservation). Memory retains last 10 user interactions.
