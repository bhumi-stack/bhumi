pub type HttpResult<E = std::io::Error> = Result<HttpResponse, E>;

pub type HttpResponse =
    hyper::Response<http_body_util::combinators::BoxBody<hyper::body::Bytes, std::io::Error>>;

pub async fn run_server(key: fastn_id52::SecretKey) -> HttpResult<HttpResponse> {
    let addr = "127.0.0.1:9000";
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => listener,
        Err(e) => panic!("failed to bind to {addr}: {e}"),
    };
    println!("Listening on http://{addr}, hub_id: {}", key.id52());
    loop {
        tokio::select! {
            val = listener.accept() => {
                match val {

                    Ok((stream, _addr)) => {
                        tokio::task::spawn(handle_connection(
                            stream, key.clone()
                        ));
                    },
                    Err(e) => {
                        eprintln!("failed to accept: {e:?}");
                        continue;
                    }
                }
            }
        }
    }
}

async fn handle_connection(stream: tokio::net::TcpStream, key: fastn_id52::SecretKey) {
    let io = hyper_util::rt::TokioIo::new(stream);

    let builder =
        hyper_util::server::conn::auto::Builder::new(hyper_util::rt::tokio::TokioExecutor::new());
    // the following builder runs only http2 service, whereas the hyper_util auto Builder runs an
    // http1.1 server that upgrades to http2 if the client requests.
    // let builder = hyper::server::conn::http2::Builder::new(hyper_util::rt::tokio::TokioExecutor::new());
    tokio::pin! {
        let conn = builder
            .serve_connection(
                io,
                // http/1.1 allows https://en.wikipedia.org/wiki/HTTP_pipelining
                // but hyper does not, https://github.com/hyperium/hyper/discussions/2747:
                //
                // > hyper does not support HTTP/1.1 pipelining, since it's a deprecated HTTP
                // > feature. it's better to use HTTP/2.
                //
                // so we will never have IN_FLIGHT_REQUESTS > OPEN_CONNECTION_COUNT.
                //
                // for hostn-edge contacting hostn-document / hostn-wasm, it may have been useful to
                // send multiple requests on the same connection as they are independent of each
                // other. without pipelining, we will end up having effectively more open
                // connections between edge and js/wasm.
                hyper::service::service_fn(|r| handle_request(r, key.clone())),
            );
    }

    if let Err(e) = tokio::select! {
        r = &mut conn => r,
    } {
        eprintln!("connection error: {e:?}");
    }
}

async fn handle_request(
    r: hyper::Request<hyper::body::Incoming>,
    key: fastn_id52::SecretKey,
) -> HttpResult {
    match r.uri().path() {
        "/_p2p" => bhumi_hub::p2p::handle(r, key).await,
        t => bhumi_hub::not_found!("not found: {t}"),
    }
}

pub fn json<T: serde::Serialize>(o: T) -> HttpResult {
    let bytes = match serde_json::to_vec(&o) {
        Ok(v) => v,
        Err(e) => return server_error_(format!("failed to serialize json: {e:?}")),
    };
    bytes_to_resp(bytes, hyper::StatusCode::OK)
}

pub fn server_error_(s: String) -> HttpResult {
    bytes_to_resp(s.into_bytes(), hyper::StatusCode::INTERNAL_SERVER_ERROR)
}

pub fn bytes_to_resp(bytes: Vec<u8>, status: hyper::StatusCode) -> HttpResult {
    use http_body_util::BodyExt;

    let mut r = hyper::Response::new(
        http_body_util::Full::new(hyper::body::Bytes::from(bytes))
            .map_err(|e| match e {})
            .boxed(),
    );
    *r.status_mut() = status;
    Ok(r)
}

pub fn not_found_(m: String) -> HttpResult {
    bytes_to_resp(m.into_bytes(), hyper::StatusCode::NOT_FOUND)
}

pub fn bad_request_(m: String) -> HttpResult {
    bytes_to_resp(m.into_bytes(), hyper::StatusCode::BAD_REQUEST)
}

#[macro_export]
macro_rules! server_error {
    ($($t:tt)*) => {{
        bhumi_hub::http::server_error_(format!($($t)*))
    }};
}

#[macro_export]
macro_rules! not_found {
    ($($t:tt)*) => {{
        bhumi_hub::http::not_found_(format!($($t)*))
    }};
}

#[macro_export]
macro_rules! bad_request {
    ($($t:tt)*) => {{
        bhumi_hub::http::bad_request_(format!($($t)*))
    }};
}

pub fn redirect<S: AsRef<str>>(url: S) -> HttpResult {
    let mut r = bytes_to_resp(vec![], hyper::StatusCode::PERMANENT_REDIRECT);
    if let Ok(ref mut r) = r {
        *r.headers_mut().get_mut(hyper::header::LOCATION).unwrap() = url.as_ref().parse().unwrap();
    }

    r
}
