//! The Java reader used when the JDK has no `jfr` tool.

/// Source of `ArborJfrDump.java`. Written to a temp dir under that exact file name (a requirement
/// of the compiled mode; the source-launcher mode does not care).
pub const HELPER_SOURCE: &str = include_str!("../java/ArborJfrDump.java");

/// File name the helper must be written as.
pub const HELPER_FILE: &str = "ArborJfrDump.java";

/// Class name for `java -cp <dir> ArborJfrDump <recording>` after compilation.
pub const HELPER_CLASS: &str = "ArborJfrDump";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_declares_the_class_it_is_launched_by() {
        assert!(HELPER_SOURCE.contains("public class ArborJfrDump"));
        assert!(HELPER_SOURCE.contains("public static void main"));
        // JDK 8 compatibility: no `var`, no text blocks, no switch expressions.
        assert!(!HELPER_SOURCE.contains(" var "));
        assert!(!HELPER_SOURCE.contains("\"\"\""));
    }
}
