#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct DependenciesInput {}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct DependenciesOutput {}

#[derive(thiserror::Error, Debug)]
pub enum GetDependenciesError {}

pub async fn get_dependencies(
    _input: DependenciesInput,
) -> Result<DependenciesOutput, GetDependenciesError> {
    todo!()
}
