//! Paths between WSL and Windows. vma.exe started from a Linux folder has
//! that folder as its current directory, the way Windows sees it,
//! `\\wsl.localhost\<distribution>\…` (`\\wsl$\…` on an older WSL); WSL
//! passes it no variable of its own. That folder is how vma knows it was
//! called from WSL and from which distribution, so a Linux path it is given
//! is found and a path it writes is shown back as Linux.
//!
//! Started from a Windows folder, `/mnt/c` included, the folder can't tell;
//! `WSL_DISTRO_NAME`, when `WSLENV` passes it, tells instead. Without either,
//! vma shows Windows paths, and takes a `/mnt/<drive>/` path but no other
//! Linux one. Windows' drives are taken to be under `/mnt`, WSL's default.

#[derive(Clone)]
pub struct Caller {
    wsl: Option<Wsl>,
}

/// The WSL distribution vma was called from, and the root its files are
/// reached by from Windows, `\\wsl.localhost\` or `\\wsl$\`.
#[derive(Clone)]
struct Wsl {
    root: &'static str,
    distro: String,
}

const UNC_ROOTS: [&str; 2] = [r"\\wsl.localhost\", r"\\wsl$\"];

impl Caller {
    /// Reads who called from the current directory, as Windows gives it,
    /// or else from `WSL_DISTRO_NAME`'s value.
    pub fn new(cwd: &str, distro_variable: Option<&str>) -> Caller {
        let from_cwd = UNC_ROOTS.iter().find_map(|&root| {
            let rest = strip_prefix_ignore_case(cwd, root)?;
            let name = rest.split('\\').next()?;
            (!name.is_empty()).then(|| Wsl {
                root,
                distro: name.to_string(),
            })
        });
        let wsl = from_cwd.or_else(|| {
            distro_variable.filter(|d| !d.is_empty()).map(|distro| Wsl {
                root: UNC_ROOTS[0],
                distro: distro.to_string(),
            })
        });
        Caller { wsl }
    }

    /// The path Windows finds `path` at. A relative path is left relative,
    /// as Windows finds it against the current directory, a Linux one too.
    pub fn windows(&self, path: &str) -> Result<String, String> {
        if !path.starts_with('/') {
            return Ok(path.to_string());
        }
        if let Some((drive, rest)) = mounted_drive(path) {
            return Ok(format!(
                "{}:\\{}",
                drive.to_ascii_uppercase(),
                rest.replace('/', "\\")
            ));
        }
        match &self.wsl {
            Some(wsl) => Ok(format!(
                "{}{}{}",
                wsl.root,
                wsl.distro,
                path.replace('/', "\\")
            )),
            None => Err(format!(
                "{path} is a Linux path, and vma can't tell which WSL it is from: run it from a Linux folder, or pass WSL_DISTRO_NAME through WSLENV"
            )),
        }
    }

    /// How a whole Windows path is shown to the caller: as Linux when it
    /// called from WSL and the path is reachable from there, as it is
    /// otherwise.
    pub fn shown(&self, path: &str) -> String {
        let Some(Wsl { distro, .. }) = &self.wsl else {
            return path.to_string();
        };
        for root in UNC_ROOTS {
            if let Some(rest) = strip_prefix_ignore_case(path, root) {
                let (name, inside) = rest.split_once('\\').unwrap_or((rest, ""));
                if name.eq_ignore_ascii_case(distro) {
                    return format!("/{}", inside.replace('\\', "/"));
                }
                return path.to_string();
            }
        }
        let bytes = path.as_bytes();
        if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            let rest = path[2..].trim_start_matches('\\');
            let drive = (bytes[0] as char).to_ascii_lowercase();
            if rest.is_empty() {
                return format!("/mnt/{drive}");
            }
            return format!("/mnt/{drive}/{}", rest.replace('\\', "/"));
        }
        path.to_string()
    }
}

/// `/mnt/c/rest` as `('c', "rest")`; `/mnt/c` as `('c', "")`.
fn mounted_drive(path: &str) -> Option<(char, &str)> {
    let rest = path.strip_prefix("/mnt/")?;
    let mut chars = rest.chars();
    let drive = chars.next().filter(char::is_ascii_alphabetic)?;
    match chars.as_str() {
        "" => Some((drive, "")),
        after => after.strip_prefix('/').map(|inside| (drive, inside)),
    }
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wsl() -> Caller {
        Caller::new(r"\\wsl.localhost\Ubuntu\home\someone\project", None)
    }

    fn windows() -> Caller {
        Caller::new(r"C:\Users\someone", None)
    }

    fn distro(caller: &Caller) -> Option<&str> {
        caller.wsl.as_ref().map(|w| w.distro.as_str())
    }

    #[test]
    fn the_distribution_comes_from_the_folder() {
        assert_eq!(distro(&wsl()), Some("Ubuntu"));
        assert_eq!(
            distro(&Caller::new(r"\\WSL$\Debian\tmp", None)),
            Some("Debian")
        );
        assert_eq!(
            distro(&Caller::new(r"\\wsl.localhost\Ubuntu", None)),
            Some("Ubuntu")
        );
        assert_eq!(distro(&windows()), None);
        assert_eq!(distro(&Caller::new(r"\\server\share", None)), None);
    }

    #[test]
    fn a_linux_path_is_found_through_the_distribution() {
        assert_eq!(
            wsl().windows("/tmp/x/b.png"),
            Ok(r"\\wsl.localhost\Ubuntu\tmp\x\b.png".into())
        );
        assert!(windows().windows("/tmp/b.png").is_err());
    }

    #[test]
    fn a_linux_path_goes_through_the_root_the_folder_came_by() {
        // An older WSL gives only \\wsl$; \\wsl.localhost may not be there.
        assert_eq!(
            Caller::new(r"\\wsl$\Debian\tmp", None).windows("/tmp/b.png"),
            Ok(r"\\wsl$\Debian\tmp\b.png".into())
        );
    }

    #[test]
    fn wsl_distro_name_tells_where_the_folder_cannot() {
        let caller = Caller::new(r"C:\Users\someone", Some("Ubuntu"));
        assert_eq!(
            caller.windows("/tmp/b.png"),
            Ok(r"\\wsl.localhost\Ubuntu\tmp\b.png".into())
        );
        assert_eq!(caller.shown(r"C:\Temp\a.png"), "/mnt/c/Temp/a.png");
        // The folder wins: it says how the files are reached.
        let caller = Caller::new(r"\\wsl$\Debian\tmp", Some("Ubuntu"));
        assert_eq!(distro(&caller), Some("Debian"));
        // Set but empty is not set.
        assert_eq!(distro(&Caller::new(r"C:\x", Some(""))), None);
    }

    #[test]
    fn a_mounted_drive_is_the_drive_itself() {
        for caller in [wsl(), windows()] {
            assert_eq!(
                caller.windows("/mnt/c/Users/a.png"),
                Ok(r"C:\Users\a.png".into())
            );
            assert_eq!(caller.windows("/mnt/d"), Ok(r"D:\".into()));
        }
        // Not a drive: a folder under /mnt whose name is longer.
        assert_eq!(
            wsl().windows("/mnt/wsl/x"),
            Ok(r"\\wsl.localhost\Ubuntu\mnt\wsl\x".into())
        );
    }

    #[test]
    fn relative_and_windows_paths_pass_as_they_are() {
        assert_eq!(wsl().windows("pics/a.png"), Ok("pics/a.png".into()));
        assert_eq!(wsl().windows(r"C:\a.png"), Ok(r"C:\a.png".into()));
    }

    #[test]
    fn written_paths_are_shown_as_linux_to_wsl() {
        assert_eq!(
            wsl().shown(r"\\wsl.localhost\Ubuntu\home\someone\pics\a.png"),
            "/home/someone/pics/a.png"
        );
        assert_eq!(wsl().shown(r"\\wsl$\ubuntu\tmp\b.png"), "/tmp/b.png");
        assert_eq!(
            wsl().shown(r"C:\Users\someone\AppData\Local\Temp\vma\1.png"),
            "/mnt/c/Users/someone/AppData/Local/Temp/vma/1.png"
        );
        // Another distribution's file is not reachable as this one's.
        assert_eq!(
            wsl().shown(r"\\wsl.localhost\Debian\tmp\c.png"),
            r"\\wsl.localhost\Debian\tmp\c.png"
        );
    }

    #[test]
    fn written_paths_stay_windows_to_windows() {
        assert_eq!(windows().shown(r"C:\Temp\a.png"), r"C:\Temp\a.png");
    }
}
