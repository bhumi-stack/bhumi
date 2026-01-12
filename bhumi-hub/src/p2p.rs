pub async fn handle(
    r: hyper::Request<hyper::body::Incoming>,
    _key: fastn_id52::SecretKey,
    home: &'static str,
) -> bhumi_hub::http::HttpResult {
    let _peer_id = match get_peer_id(&r) {
        Ok(peer_id) => peer_id,
        Err(e) => return bhumi_hub::bad_request!("invalid cookie header: {e}"),
    };

    let body = match http_body_util::BodyExt::collect(r.into_body()).await {
        Ok(body) => body.to_bytes(),
        Err(e) => return bhumi_hub::bad_request!("failed to read body: {e}"),
    };

    match serde_json::from_slice::<Command>(&body)? {
        Command::Render(path) => match bhumi_hub::render(&path, home).await {
            Ok(o) => bhumi_hub::http::json(o),
            Err(e) => bhumi_hub::bad_request!("failed to render: {e}"),
        },
        Command::GetDependencies(input) => match bhumi_hub::get_dependencies(input).await {
            Ok(o) => bhumi_hub::http::json(o),
            Err(e) => bhumi_hub::bad_request!("failed to get dependencies: {e}"),
        },
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum Command {
    Render(String),
    GetDependencies(bhumi_hub::DependenciesInput),
}

#[derive(Debug, thiserror::Error)]
pub enum PeerIdError {
    #[error("invalid cookie header: {e}")]
    InvalidHeaderValue { e: hyper::http::header::ToStrError },
    #[error("PeerIdCookieMissing")]
    PeerIdCookieMissing,
    #[error("SignatureCookieMissing")]
    SignatureCookieMissing,
    #[error("verification failed")]
    VerificationFailed(fastn_id52::SignatureVerificationError),
}

fn get_peer_id(
    r: &hyper::Request<hyper::body::Incoming>,
) -> Result<fastn_id52::PublicKey, PeerIdError> {
    let (peer_id, signature): (fastn_id52::PublicKey, fastn_id52::Signature) = {
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
        (
            std::str::FromStr::from_str(&peer_id.unwrap()).unwrap(),
            std::str::FromStr::from_str(&signature.unwrap()).unwrap(),
        )
    };

    // FIXME(security): for now the signature is that of the peer_id itself,
    //                  before going live we will add replay attack mitigation
    peer_id
        .verify(peer_id.to_string().as_bytes(), &signature)
        .map_err(PeerIdError::VerificationFailed)?;

    Ok(peer_id)
}
