use std::fs;
use std::path::Path;

use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyroadsCfg {
    pub raw: Vec<u8>,
}

impl SkyroadsCfg {
    pub fn byte_count(&self) -> usize {
        self.raw.len()
    }
}

pub fn load_cfg_path(path: impl AsRef<Path>) -> Result<SkyroadsCfg> {
    let data = fs::read(path)?;
    Ok(load_cfg_bytes(&data))
}

pub fn load_cfg_bytes(data: &[u8]) -> SkyroadsCfg {
    SkyroadsCfg { raw: data.to_vec() }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::load_cfg_path;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn shipped_cfg_loads_as_raw_bytes() {
        let cfg_path = repo_root().join("SKYROADS.CFG");
        if cfg_path.exists() {
            let cfg = load_cfg_path(cfg_path).unwrap();
            assert!(cfg.byte_count() > 0);
        } else {
            let cfg = super::load_cfg_bytes(&[1, 2, 3, 4]);
            assert_eq!(cfg.byte_count(), 4);
        }
    }
}
