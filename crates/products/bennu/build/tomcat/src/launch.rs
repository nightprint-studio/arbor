//! The JVM command line of a Tomcat run.
//!
//! This is what `catalina.bat run` assembles, minus the script: the same system properties and the
//! same `Bootstrap start`, so the console shows what Tomcat's own launcher would have run. Starting
//! the JVM directly is what lets the backend's ordinary run machinery own the process — console,
//! stdin, Stop, the debugger — instead of a shell wrapper that hides the real PID.

use crate::context::secret_property;

/// What the command line is built from.
#[derive(Debug, Clone)]
pub struct LaunchInput {
    /// `CATALINA_HOME` — the installation.
    pub home: String,
    /// `CATALINA_BASE` — the generated instance.
    pub base: String,
    /// The user's JVM options (`-Xmx1g -Dfile.encoding=UTF-8`), already split.
    pub jvm_args: Vec<String>,
    /// `(resource name, password)` for every data source whose password is a secret. Handed over as
    /// `-D<property>=<value>` — see [`crate::context`].
    pub secrets: Vec<(String, String)>,
    /// The JDK's major version, which decides whether the `--add-opens` Tomcat needs on Java 9+ are
    /// passed (JDK 8 rejects them).
    pub jdk_major: u32,
    /// The classpath separator of the machine that will run it.
    pub path_separator: char,
}

/// The arguments after the `java` program, and the same line for display with secrets masked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    pub args: Vec<String>,
    pub display: Vec<String>,
}

/// What `catalina.sh` adds on Java 9+ so Tomcat's reflection into the JDK keeps working.
const ADD_OPENS: [&str; 5] = [
    "--add-opens=java.base/java.lang=ALL-UNNAMED",
    "--add-opens=java.base/java.io=ALL-UNNAMED",
    "--add-opens=java.base/java.util=ALL-UNNAMED",
    "--add-opens=java.base/java.util.concurrent=ALL-UNNAMED",
    "--add-opens=java.rmi/sun.rmi.transport=ALL-UNNAMED",
];

/// Assemble the arguments for `java`. A debugger's agent argument, when there is one, is the
/// caller's to put first.
pub fn java_args(input: &LaunchInput) -> LaunchPlan {
    let mut args: Vec<String> = Vec::new();
    if input.jdk_major >= 9 {
        args.extend(ADD_OPENS.iter().map(|s| s.to_string()));
    }
    args.push(format!("-Dcatalina.home={}", input.home));
    args.push(format!("-Dcatalina.base={}", input.base));
    args.push(format!("-Djava.io.tmpdir={}/temp", input.base));
    args.push("-Djava.util.logging.manager=org.apache.juli.ClassLoaderLogManager".to_string());
    args.push(format!("-Djava.util.logging.config.file={}/conf/logging.properties", input.base));
    args.extend(input.jvm_args.iter().cloned());

    let mut display = args.clone();
    for (name, password) in &input.secrets {
        let property = secret_property(name);
        args.push(format!("-D{property}={password}"));
        display.push(format!("-D{property}=••••••"));
    }

    let classpath = format!(
        "{home}/bin/bootstrap.jar{sep}{home}/bin/tomcat-juli.jar",
        home = input.home,
        sep = input.path_separator
    );
    for tail in [vec!["-cp".to_string(), classpath], vec!["org.apache.catalina.startup.Bootstrap".to_string(), "start".to_string()]] {
        args.extend(tail.iter().cloned());
        display.extend(tail);
    }
    LaunchPlan { args, display }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(jdk: u32, secrets: Vec<(String, String)>) -> LaunchInput {
        LaunchInput {
            home: "/t/home".into(),
            base: "/t/base".into(),
            jvm_args: vec!["-Xmx1g".into()],
            secrets,
            jdk_major: jdk,
            path_separator: ':',
        }
    }

    #[test]
    fn the_line_names_home_base_and_the_bootstrap() {
        let p = java_args(&input(17, vec![]));
        assert!(p.args.contains(&"-Dcatalina.base=/t/base".to_string()));
        assert!(p.args.contains(&"-Xmx1g".to_string()));
        let n = p.args.len();
        assert_eq!(p.args[n - 4..], ["-cp", "/t/home/bin/bootstrap.jar:/t/home/bin/tomcat-juli.jar", "org.apache.catalina.startup.Bootstrap", "start"]);
    }

    #[test]
    fn add_opens_is_for_java_nine_and_later_only() {
        assert!(java_args(&input(17, vec![])).args.iter().any(|a| a.starts_with("--add-opens")));
        assert!(!java_args(&input(8, vec![])).args.iter().any(|a| a.starts_with("--add-opens")));
    }

    #[test]
    fn a_secret_reaches_the_process_and_never_the_display() {
        let p = java_args(&input(17, vec![("jdbc/G".into(), "s3cret".into())]));
        assert!(p.args.contains(&"-Djdbc.G.password=s3cret".to_string()));
        assert!(!p.display.iter().any(|a| a.contains("s3cret")));
        assert!(p.display.contains(&"-Djdbc.G.password=••••••".to_string()));
    }
}
