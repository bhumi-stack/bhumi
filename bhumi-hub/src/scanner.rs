/// we scan the entire BHUMI_HOME/repo and find all the .scm files, and their
/// direct dependencies, e.g. if a/b.scm imports foo.scm and bar.scm we will
/// store foo.scm and bar.scm, and we also store the hash and content of each
/// file in memory
pub struct ScanData {
    modules: std::collections::HashMap<String, Module>,
}

pub struct Module {
    source: String,
    content_hash: String,      // sha sum
    dependencies: Vec<String>, // path relative to BHUMI_HOME/repo
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct RescanOutput {}

#[derive(thiserror::Error, Debug)]
pub enum RescanError {}

pub async fn rescan(_home: &str) -> Result<RescanOutput, RescanError> {
    todo!()
}
