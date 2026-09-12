//! Where a generated validation test goes.

/// `Order` → `OrderValidationTest`.
pub fn test_class_name(class_name: &str) -> String {
    format!("{class_name}ValidationTest")
}

/// The test file for a class: the mirror of `src/main/java` under `src/test/java` in a Maven layout,
/// beside the class otherwise.
pub fn test_file_for(source_file: &str, package: &str, test_class: &str) -> String {
    let normalized = source_file.replace('\\', "/");
    let file = format!("{test_class}.java");
    if let Some((module, _)) = normalized.split_once("/src/main/java/") {
        let package_dir = package.replace('.', "/");
        return [module, "src/test/java", package_dir.as_str(), file.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<&str>>()
            .join("/");
    }
    match normalized.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{file}"),
        None => file,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_maven_class_gets_its_test_under_src_test_java() {
        assert_eq!(
            test_file_for("/work/shop/orders/src/main/java/com/example/Order.java", "com.example", "OrderValidationTest"),
            "/work/shop/orders/src/test/java/com/example/OrderValidationTest.java"
        );
        assert_eq!(test_file_for("/tmp/Order.java", "", "OrderValidationTest"), "/tmp/OrderValidationTest.java");
    }
}
