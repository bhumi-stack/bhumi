pub async fn handle(r: hyper::Request<hyper::body::Incoming>) -> bhumi_hub::http::HttpResult {
    let _peer_id = match get_peer_id(&r) {
        Ok(peer_id) => peer_id,
        Err(e) => return bhumi_hub::bad_request!("invalid cookie header: {e}"),
    };

    let body = match http_body_util::BodyExt::collect(r.into_body()).await {
        Ok(body) => body.to_bytes(),
        Err(e) => return bhumi_hub::bad_request!("failed to read body: {e}"),
    };

    let _command = serde_json::from_slice::<Command>(&body)?;

    todo!()
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum Command {}

#[derive(Debug, thiserror::Error)]
pub enum PeerIdError {
    #[error("invalid cookie header: {e}")]
    InvalidHeaderValue { e: hyper::http::header::ToStrError },
    #[error("PeerIdCookieMissing")]
    PeerIdCookieMissing,
    #[error("SignatureCookieMissing")]
    SignatureCookieMissing,
}

fn get_peer_id(r: &hyper::Request<hyper::body::Incoming>) -> Result<String, PeerIdError> {
    let (peer_id, signature) = {
        let mut peer_id = None;
        let mut signature = None;
        if let Some(cookie_header) = r.headers().get(hyper::header::COOKIE) {
            let cookie_str = match cookie_header.to_str() {
                Ok(cookie_str) => cookie_str,
                Err(e) => return Err(PeerIdError::InvalidHeaderValue { e }),
            };

            for cookie_part in cookie_str.split(';') {
                if let Ok(cookie) = cookie::Cookie::parse(cookie_part.trim()) {
                    println!("cookie parse: {cookie:?}");
                    match cookie.name() {
                        "peer_id" => peer_id = Some(cookie.value().to_string()),
                        "signature" => signature = Some(cookie.value().to_string()),
                        _ => {}
                    }
                }
            }
        }
        if peer_id.is_none() {
            return Err(PeerIdError::PeerIdCookieMissing);
        }
        if signature.is_none() {
            return Err(PeerIdError::SignatureCookieMissing);
        }
        (peer_id.unwrap(), signature.unwrap())
    };

    // todo: signature verification
    Ok(peer_id)
}
