//! The Tomcat release lines Bennu can run, and where a release is published.
//!
//! The one fact that decides whether an application starts is the **servlet namespace**: Tomcat 9
//! and earlier speak `javax.servlet`, Tomcat 10 and later `jakarta.servlet`, and an application
//! compiled for one does not load on the other. It is carried here, per line, so a mismatch is a
//! sentence before the launch rather than a `ClassNotFoundException` after it.

/// One release line (`9.0`) and what it speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line {
    /// `"9.0"`, `"10.1"`… — the version prefix every release of the line shares.
    pub line: &'static str,
    /// The servlet package root: `"javax"` or `"jakarta"`.
    pub namespace: &'static str,
    /// The Servlet specification it implements.
    pub servlet: &'static str,
    /// The oldest Java it runs on.
    pub min_java: u32,
}

/// The supported lines, newest first. 8.0 is not here: it is no longer distributed outside the
/// archive, and 8.5 is the line that replaced it.
pub const LINES: [Line; 4] = [
    Line { line: "11.0", namespace: "jakarta", servlet: "6.1", min_java: 17 },
    Line { line: "10.1", namespace: "jakarta", servlet: "6.0", min_java: 11 },
    Line { line: "9.0", namespace: "javax", servlet: "4.0", min_java: 8 },
    Line { line: "8.5", namespace: "javax", servlet: "3.1", min_java: 7 },
];

/// The line a version (`9.0.98`) belongs to.
pub fn line_of(version: &str) -> Option<&'static Line> {
    LINES.iter().find(|l| version.strip_prefix(l.line).is_some_and(|rest| rest.starts_with('.')))
}

fn major(line: &str) -> &str {
    line.split('.').next().unwrap_or(line)
}

const ARCHIVE: &str = "https://archive.apache.org/dist/tomcat";

/// The directory listing of every release of `line` — what [`parse_listing`] reads.
pub fn listing_url(line: &str) -> String {
    format!("{ARCHIVE}/tomcat-{}/", major(line))
}

/// The `.zip` of `version`, or `None` for a version no known line contains.
pub fn archive_url(version: &str) -> Option<String> {
    let line = line_of(version)?;
    Some(format!("{ARCHIVE}/tomcat-{}/v{version}/bin/apache-tomcat-{version}.zip", major(line.line)))
}

/// The SHA-512 file published beside [`archive_url`].
pub fn checksum_url(version: &str) -> Option<String> {
    archive_url(version).map(|u| format!("{u}.sha512"))
}

fn numeric_triplet(v: &str) -> Option<(u32, u32, u32)> {
    let mut parts = v.split('.');
    let t = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    parts.next().is_none().then_some(t)
}

/// The stable releases of `line` found in an Apache directory listing, newest first.
///
/// A release candidate or milestone (`10.1.0-M1`) is not a `x.y.z` and is dropped: the listing is
/// where those live too, and offering one as "a version of Tomcat 10.1" is how somebody ends up
/// running a build nobody called finished.
pub fn parse_listing(html: &str, line: &str) -> Vec<String> {
    let prefix = format!("{line}.");
    let mut found: Vec<((u32, u32, u32), String)> = html
        .split("href=\"v")
        .skip(1)
        .filter_map(|chunk| chunk.split("/\"").next())
        .filter(|v| v.starts_with(&prefix))
        .filter_map(|v| numeric_triplet(v).map(|t| (t, v.to_string())))
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.dedup_by(|a, b| a.1 == b.1);
    found.into_iter().map(|(_, v)| v).collect()
}

/// The digest in an Apache `.sha512` file (`<hex>  <file name>`), lowercased.
pub fn parse_sha512(text: &str) -> Option<String> {
    let first = text.split_whitespace().next()?;
    (first.len() == 128 && first.bytes().all(|b| b.is_ascii_hexdigit())).then(|| first.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_belongs_to_its_line_and_a_longer_prefix_does_not_confuse_it() {
        assert_eq!(line_of("9.0.98").unwrap().namespace, "javax");
        assert_eq!(line_of("10.1.34").unwrap().namespace, "jakarta");
        assert_eq!(line_of("8.5.100").unwrap().min_java, 7);
        // `10.1` must not be read as `1`-something, nor `11.0` as `1`.
        assert!(line_of("1.0.0").is_none());
        assert!(line_of("9.00.1").is_none());
    }

    #[test]
    fn a_release_is_published_under_its_majors_directory() {
        assert_eq!(
            archive_url("10.1.34").unwrap(),
            "https://archive.apache.org/dist/tomcat/tomcat-10/v10.1.34/bin/apache-tomcat-10.1.34.zip"
        );
        assert!(checksum_url("9.0.98").unwrap().ends_with("apache-tomcat-9.0.98.zip.sha512"));
        assert!(archive_url("7.0.1").is_none());
        assert_eq!(listing_url("8.5"), "https://archive.apache.org/dist/tomcat/tomcat-8/");
    }

    #[test]
    fn the_listing_yields_stable_releases_of_the_line_newest_first() {
        let html = r#"<a href="v9.0.9/">v9.0.9/</a><a href="v9.0.98/">v9.0.98/</a><a href="v9.0.0.M1/">x</a>
            <a href="v9.0.100/">v9.0.100/</a><a href="v10.1.1/">v10.1.1/</a><a href="v9.0.98/">again</a>"#;
        assert_eq!(parse_listing(html, "9.0"), ["9.0.100", "9.0.98", "9.0.9"]);
        assert_eq!(parse_listing(html, "10.1"), ["10.1.1"]);
    }

    #[test]
    fn a_checksum_file_is_a_hex_digest_then_a_name() {
        let digest = "a".repeat(128);
        assert_eq!(parse_sha512(&format!("{digest}  apache-tomcat-9.0.98.zip\n")), Some(digest));
        assert_eq!(parse_sha512("not a digest  file.zip"), None);
        assert_eq!(parse_sha512(&"g".repeat(128)), None);
    }
}
