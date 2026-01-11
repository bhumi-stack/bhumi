pub async fn handle(r: hyper::Request<hyper::body::Incoming>) -> bhumi_hub::http::HttpResult {
    let body = match http_body_util::BodyExt::collect(r.into_body()).await {
        Ok(body) => body.to_bytes(),
        Err(e) => return bhumi_hub::bad_request!("failed to read body: {e}"),
    };
    let _command = serde_json::from_slice::<Command>(&body)?;

    todo!()
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum Command {}
