#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct RenderOutput {}

#[derive(thiserror::Error, Debug)]
pub enum RenderError {}

pub async fn render(_path: &str, home: &str) -> Result<RenderOutput, RenderError> {
    // the path is $BHUMI_HOME/repo/<file>.scm
    todo!()
}

fn path_candidates(path: &str, home: &str) -> (String, String) {
    let path = path.trim_end_matches('/');
    (
        format!("{home}{path}.scm"),
        format!("{home}{path}/index.scm"),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn path_candidates() {
        assert_eq!(
            super::path_candidates("/foo", ".bhumi"),
            (
                ".bhumi/foo.scm".to_string(),
                ".bhumi/foo/index.scm".to_string()
            )
        );
    }
}
