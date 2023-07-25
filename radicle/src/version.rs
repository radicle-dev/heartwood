use std::io;

/// Print program version.
///
/// The program version follows [semantic versioning](https://semver.org).
///
/// Adjust with caution, third party applications parse the string for version info.
pub fn print(
    mut w: impl std::io::Write,
    name: &str,
    version: &str,
    git_head: &str,
) -> Result<(), io::Error> {
    if version.ends_with("-dev") {
        writeln!(w, "{name} {version}+{git_head}")?;
    } else {
        writeln!(w, "{name} {version} ({git_head})")?;
    };
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_version() {
        let mut buffer = Vec::new();
        print(&mut buffer, "rad", "1.2.3", "28b341d").unwrap();
        let res = std::str::from_utf8(&buffer).unwrap();
        assert_eq!("rad 1.2.3 (28b341d)\n", res);

        let mut buffer = Vec::new();
        print(&mut buffer, "rad", "1.2.3-dev", "28b341d").unwrap();
        let res = std::str::from_utf8(&buffer).unwrap();
        assert_eq!("rad 1.2.3-dev+28b341d\n", res);
    }
}
