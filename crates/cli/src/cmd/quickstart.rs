use std::io::{self, Write};

pub fn cmd_quickstart(_args: &[&str]) {
    print_quickstart(&mut io::stdout()).expect("Failed to write quickstart guide");
}

fn print_quickstart(mut writer: impl Write) -> io::Result<()> {
    writeln!(writer, "🚀 RoCo Quickstart Guide")?;
    writeln!(writer, "=====================\n")?;
    writeln!(writer, "1. GPU Requirements:")?;
    writeln!(writer, "   - Vulkan support is required.")?;
    writeln!(
        writer,
        "   - ~6GB VRAM is needed for the default 2.9B model.\n"
    )?;
    writeln!(writer, "2. Environment Setup:")?;
    writeln!(
        writer,
        "   Download the model and vocab, then set the following variables:"
    )?;
    writeln!(writer, "   export RWKV_MODEL=\"/path/to/model.st\"")?;
    writeln!(writer, "   export RWKV_VOCAB=\"/path/to/vocab.json\"\n")?;
    writeln!(writer, "3. Start the Inference Daemon:")?;
    writeln!(writer, "   roco inferd start\n")?;
    writeln!(writer, "4. Create a Story (One-command):")?;
    writeln!(
        writer,
        "   roco story --premise \"A cyberpunk detective investigates a rogue AI\"\n"
    )?;
    writeln!(writer, "5. Where Stories are Saved:")?;
    writeln!(
        writer,
        "   Your generated stories are saved in the `.roco/stories/` directory."
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quickstart_output() {
        let mut buf = Vec::new();
        print_quickstart(&mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains("Quickstart"));
        assert!(output.contains("GPU"));
        assert!(output.contains("~6GB VRAM"));
        assert!(output.contains("RWKV_MODEL"));
        assert!(output.contains("RWKV_VOCAB"));
        assert!(output.contains("roco inferd start"));
        assert!(output.contains("roco story"));
        assert!(output.contains(".roco/stories/"));
    }
}
