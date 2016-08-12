
// This file deals with XML-RPC, thank you https://github.com/joeyfeldberg

use hyper;
use std::io::Read;
use xmlrpc::{Request, Response};
use error::SubError;

const OPENSUBTITLES_SERVER: &'static str = "http://api.opensubtitles.org/xml-rpc";

pub struct RpcClient {
    url: String,
}

// If using this Client from xml-rpc the From<hyper::Error> conversion doesn't work in OpenSubtitlesClient.
impl RpcClient {
    pub fn new(s: &str) -> RpcClient {
        RpcClient { url: s.to_string() }
    }

    pub fn remote_call(&self, request: &Request) -> Result<Response, hyper::Error> {
        let http_client = hyper::Client::new();
        let mut result = try!(http_client.post(&self.url).body(&request.body).send());

        let mut body = String::new();

        try!(result.read_to_string(&mut body));
        Ok(Response::new(&body))
    }
}

pub struct OpenSubtitlesClient {
    token: String,
    client: RpcClient,
}

#[allow(non_snake_case)]
#[derive(RustcEncodable)]
struct SubtitlesQuery {
    sublanguageid: String,
    moviehash: String,
    moviebytesize: String,
}

#[derive(RustcDecodable)]
struct TokenResponse {
    token: String,
    status: String,
}

#[allow(non_snake_case)]
#[derive(RustcDecodable, Debug)]
pub struct SubtitleSearchResponse {
    pub IDSubMovieFile: String,
    pub ZipDownloadLink: String,
}

#[derive(RustcDecodable)]
struct SubtitleSearchResponseWrapper {
    status: String,
    data: Vec<SubtitleSearchResponse>,
}

macro_rules! prase_response {
    ($response:expr) => {
      match $response {
        Ok(mut list) => list.pop().unwrap(),
        Err(_) => return Err(SubError::SvrInvalidResponse)
      }
    };
}

impl OpenSubtitlesClient {
    pub fn create_client(username: &str, password: &str, lang: &str, useragent: &str)
                         -> Result<OpenSubtitlesClient, SubError> {
        let client = RpcClient::new(OPENSUBTITLES_SERVER);
        let mut request = Request::new("LogIn");
        request = request.argument(&username).argument(&password).argument(&lang)
                         .argument(&useragent).finalize();

        let resp = try!(client.remote_call(&request));
        let res: TokenResponse = prase_response!(resp.result::<TokenResponse>());
        if res.status.starts_with("200") {
            Ok(OpenSubtitlesClient { token: res.token, client: client, })
        } else {
            Err(SubError::SvrInvalidCredentials)
        }
    }

    pub fn search_subtitles(&self, hash: &str, size: u64, lang: &str)
                            -> Result<Vec<SubtitleSearchResponse>, SubError> {
        let mut request = Request::new("SearchSubtitles");
        let size_str = size.to_string();
        let query = SubtitlesQuery {
            sublanguageid: lang.into(),
            moviehash: hash.into(),
            moviebytesize: size_str,
        };
        request = request.argument(&self.token).argument(&[query]).finalize();

        let resp = try!(self.client.remote_call(&request));
        let res: SubtitleSearchResponseWrapper =
            prase_response!(resp.result::<SubtitleSearchResponseWrapper>());

        if res.status.starts_with("200") {
            Ok(res.data)
        } else {
            Err(SubError::SvrNoSubtitlesFound)
        }
    }
}

#[test]
fn test_bad_login() {
    let res = OpenSubtitlesClient::create_client("fakeuser", "fakepassword", "fakelang", "qwe");
    assert_eq!(res.err().unwrap(), SubError::InvalidCredentials);
}

#[test]
fn test_good_login() {
    let res = OpenSubtitlesClient::create_client("", "", "eng", "OSTestUserAgent");
    assert!(res.is_ok());
}
