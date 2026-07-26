//! GBNF ↔ kbnf format conversion utilities.
//!
//! kbnf's GBNF dialect uses `;` to terminate rules, while llama.cpp GBNF
//! uses newlines. [`gbnf_to_kbnf`] converts between the two.
//!
//! This is the replacement for the old `BnfConstraint` + schoolmarm fallback
//! architecture which was removed. All grammar enforcement is now done
//! through `roco_bnf_engine::BnfEngine` (wrapping kbnf).

pub use crate::kbnf_compat::gbnf_to_kbnf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adds_semicolons() {
        let input = concat!(
            "string ::= \"\\\"\" (char | escape)* \"\\\"\"\n",
            "char ::= \"a\" | \"b\"\n",
            "root ::= string\n",
        );
        let out = gbnf_to_kbnf(input);
        for line in out.lines() {
            let t = line.trim();
            if t.contains("::=") && !t.is_empty() {
                assert!(t.ends_with(';'), "rule should end with ;: {t:?}");
            }
        }
    }

    #[test]
    fn test_preserves_existing_semicolons() {
        let input = "root ::= \"yes\" | \"no\";\n";
        let out = gbnf_to_kbnf(input);
        assert_eq!(out, "root ::= \"yes\" | \"no\";\n");
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(gbnf_to_kbnf(""), "");
    }
}
