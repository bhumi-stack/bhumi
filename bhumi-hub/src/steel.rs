pub async fn handle(_r: hyper::Request<hyper::body::Incoming>) -> bhumi_hub::http::HttpResult {
    todo!()
    // match r.uri().path() {
    //     "/api/deps" => {
    //         // we fetch dependencies here. _access.scm acts as ACL
    //         deps()
    //     }
    //     _ => {
    //         // for any other path we check if there is a .scp file on it, if we we execute
    //         // it and send the result
    //     }
    // }
}
