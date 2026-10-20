use std::{io, path::PathBuf};

pub struct Consent {
    pub text: String,
    pub accepted: bool,
    record: Option<PathBuf>,
}

fn disclaimer() -> String {
    include_str!("../README.md")
        .split("## 免责声明")
        .nth(1)
        .expect("README must contain the disclaimer")
        .split("\n## ")
        .next()
        .unwrap()
        .trim()
        .replace("\r\n", "\n")
        .replace("**", "")
        .replace("[LICENSE](LICENSE)", "LICENSE")
}

impl Consent {
    pub fn load() -> Self {
        let record = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .map(|base| base.join("UnblockMCDownload").join("consent.txt"));
        Self::from_record(record)
    }

    fn from_record(record: Option<PathBuf>) -> Self {
        let text = disclaimer();
        let accepted = record
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .is_some_and(|saved| saved == text);
        Self {
            text,
            accepted,
            record,
        }
    }

    pub fn accept(&mut self) -> io::Result<()> {
        let record = self
            .record
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "无法定位用户配置目录"))?;
        std::fs::create_dir_all(record.parent().unwrap())?;
        std::fs::write(record, &self.text)?;
        self.accepted = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_explicit_acceptance_and_matches_current_terms() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config/consent.txt");
        let mut consent = Consent::from_record(Some(path.clone()));
        assert!(!consent.accepted);
        assert!(consent.text.contains("1. 使用目的"));
        assert!(consent.text.contains("4. 独立项目"));
        assert!(!consent.text.contains("## 许可证"));
        consent.accept().unwrap();
        assert!(Consent::from_record(Some(path.clone())).accepted);
        std::fs::write(&path, "旧条款").unwrap();
        assert!(!Consent::from_record(Some(path)).accepted);
    }

    #[test]
    fn cannot_continue_when_record_cannot_be_saved() {
        let mut consent = Consent::from_record(None);
        assert!(consent.accept().is_err());
        assert!(!consent.accepted);
    }
}
