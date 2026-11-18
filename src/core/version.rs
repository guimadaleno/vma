//! Which VirtualBox vma works with. It calls VirtualBox by name, which keeps
//! it apart from the interfaces' IDs, but the numbers of VirtualBox's enums
//! and the arguments of its methods are those of the version they were read
//! from, and they change: a running machine is 5 in 6.1 and 6 from 7.0. So a
//! VirtualBox vma was not checked against is refused rather than misread.

/// The oldest VirtualBox vma was checked against, and the major version it
/// stays within, since a new major version is where VirtualBox renumbers.
const OLDEST: (u32, u32) = (7, 2);

/// Takes VirtualBox's version as it gives it, as `7.2.8` or
/// `7.2.8_Ubuntu`.
pub fn check(version: &str) -> Result<(), String> {
    let mut parts = version
        .split(|c: char| !c.is_ascii_digit())
        .map(|p| p.parse::<u32>().ok());
    let (major, minor) = (parts.next().flatten(), parts.next().flatten());
    match (major, minor) {
        (Some(major), Some(minor)) if major == OLDEST.0 && minor >= OLDEST.1 => Ok(()),
        _ => Err(format!(
            "vma works with VirtualBox {}.{} and later {}.x, and this is VirtualBox {version}",
            OLDEST.0, OLDEST.1, OLDEST.0
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seven_two_and_later_sevens_are_taken() {
        for v in ["7.2.8", "7.2.0", "7.2.8_Ubuntu", "7.3.0", "7.10.1r1"] {
            assert_eq!(check(v), Ok(()), "{v}");
        }
    }

    #[test]
    fn others_are_refused_by_name() {
        for v in ["7.1.18", "7.0.0", "6.1.50", "8.0.0", "", "seven"] {
            assert!(check(v).is_err(), "{v}");
        }
        assert_eq!(
            check("6.1.50"),
            Err(
                "vma works with VirtualBox 7.2 and later 7.x, and this is VirtualBox 6.1.50".into()
            )
        );
    }
}
