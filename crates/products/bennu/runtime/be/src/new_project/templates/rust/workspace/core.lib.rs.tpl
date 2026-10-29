pub fn greeting(who: &str) -> String {
    format!("Hello, {who}!")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greets() {
        assert_eq!(greeting("world"), "Hello, world!");
    }
}
