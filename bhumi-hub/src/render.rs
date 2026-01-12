#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct RenderOutput {}

#[derive(thiserror::Error, Debug)]
pub enum RenderError {}

pub async fn render(_path: &str) -> Result<RenderOutput, RenderError> {
    todo!()
}
