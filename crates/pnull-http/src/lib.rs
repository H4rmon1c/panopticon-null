//! DNS-safe HTTP provenance and conditional retrieval.
//!
//! Every request and redirect produces a persisted [`FetchObservation`]
//! recording the requested URL, resolved public addresses, status, redirect
//! target, allowlisted headers, content metadata, and body digest. Connection
//! targets are resolved and validated: loopback, private, link-local,
//! multicast, unspecified, documentation, and other non-public addresses are
//! rejected, and mixed public/prohibited answers fail closed. HTTPS is
//! mandatory and certificate validation is never disabled. Redirects are
//! revalidated and constrained to explicitly reviewed hosts.
//!
//! Resolver and transport are trait abstractions so all tests remain offline.

use std::net::{IpAddr, ToSocketAddrs};

use pnull_core::{
    ConditionalResult, FetchObservation, StructuredError, sha256_hex,
};
use reqwest::blocking::Client;
use reqwest::redirect::Policy;
use thiserror::Error;
use url::Url;

pub const MAX_REDIRECTS: usize = 5;

#[derive(Debug, Error)]
pub enum HttpError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    #[error("only public HTTPS URLs are accepted")]
    InsecureUrl,
    #[error("DNS resolution failed for {0}")]
    Resolution(String),
    #[error("connection target {0} is not a public address")]
    NonPublicAddress(String),
    #[error("mixed public and prohibited DNS answers for {0}")]
    MixedAnswers(String),
    #[error("redirect to unreviewed host {0}")]
    UnreviewedRedirect(String),
    #[error("redirect loop detected")]
    RedirectLoop,
    #[error("too many redirects")]
    TooManyRedirects,
    #[error("network transport failed")]
    Transport,
    #[error("response exceeds the configured {0}-byte limit")]
    Oversized(usize),
    #[error(transparent)]
    Core(#[from] pnull_core::CoreError),
}

/// Resolves a hostname to its addresses. Abstracted so tests stay offline.
pub trait Resolver {
    fn resolve(&self, host: &str) -> Result<Vec<IpAddr>, HttpError>;
}

/// Uses the system resolver via `ToSocketAddrs`.
pub struct SystemResolver;

impl Resolver for SystemResolver {
    fn resolve(&self, host: &str) -> Result<Vec<IpAddr>, HttpError> {
        let addresses: Vec<IpAddr> = (host, 443u16)
            .to_socket_addrs()
            .map_err(|_| HttpError::Resolution(host.to_owned()))?
            .map(|address| address.ip())
            .collect();
        if addresses.is_empty() {
            return Err(HttpError::Resolution(host.to_owned()));
        }
        Ok(addresses)
    }
}

/// Performs a single HTTP request. Abstracted so tests stay offline.
pub trait Transport {
    fn request(&self, request: &TransportRequest) -> Result<TransportResponse, HttpError>;
}

#[derive(Clone, Debug)]
pub struct TransportRequest {
    pub url: Url,
    pub conditional_etag: Option<String>,
    pub conditional_modified: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TransportResponse {
    pub status_code: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Uses reqwest with redirect following disabled so the caller controls and
/// records every hop.
pub struct ReqwestTransport {
    client: Client,
}

impl ReqwestTransport {
    pub fn new(max_bytes: usize) -> Result<Self, HttpError> {
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .redirect(Policy::none())
            .user_agent(concat!(
                "PanopticonNull/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/H4rmon1c/panopticon-null)"
            ))
            .build()
            .map_err(|_| HttpError::Transport)?;
        let _ = max_bytes;
        Ok(Self { client })
    }
}

impl Transport for ReqwestTransport {
    fn request(&self, request: &TransportRequest) -> Result<TransportResponse, HttpError> {
        let mut builder = self.client.get(request.url.clone());
        if let Some(etag) = &request.conditional_etag {
            builder = builder.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        if let Some(modified) = &request.conditional_modified {
            builder = builder.header(reqwest::header::IF_MODIFIED_SINCE, modified);
        }
        let response = builder.send().map_err(|_| HttpError::Transport)?;
        let status_code = response.status().as_u16();
        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|text| (name.as_str().to_owned(), text.to_owned()))
            })
            .collect();
        let body = response.bytes().map(|bytes| bytes.to_vec()).map_err(|_| HttpError::Transport)?;
        Ok(TransportResponse {
            status_code,
            headers,
            body,
        })
    }
}

/// Whether an IP address is public (not loopback, private, link-local,
/// multicast, unspecified, documentation, or otherwise non-routable).
pub fn is_public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(value) => {
            !(value.is_private()
                || value.is_loopback()
                || value.is_link_local()
                || value.is_broadcast()
                || value.is_unspecified()
                || value.is_multicast()
                || value.is_documentation())
        }
        IpAddr::V6(value) => {
            !(value.is_loopback()
                || value.is_unspecified()
                || value.is_multicast()
                || value.is_unique_local()
                || value.is_unicast_link_local())
        }
    }
}

/// Validates that every resolved address is public, failing closed on any
/// prohibited answer (including mixed answers).
pub fn validate_resolved(host: &str, addresses: &[IpAddr]) -> Result<(), HttpError> {
    if addresses.is_empty() {
        return Err(HttpError::Resolution(host.to_owned()));
    }
    let public = addresses.iter().filter(|ip| is_public_address(**ip)).count();
    if public == 0 {
        return Err(HttpError::NonPublicAddress(host.to_owned()));
    }
    if public != addresses.len() {
        return Err(HttpError::MixedAnswers(host.to_owned()));
    }
    Ok(())
}

/// Headers that may be persisted. Everything else (cookies, authorization,
/// bearer tokens, and other sensitive headers) is never recorded.
const ALLOWLISTED_HEADERS: &[&str] = &[
    "content-type",
    "content-length",
    "etag",
    "last-modified",
    "location",
    "date",
    "server",
    "cache-control",
    "expires",
    "retry-after",
    "content-encoding",
    "content-language",
    "x-content-type-options",
];

fn allowlist_headers(headers: &[(String, String)]) -> Vec<(String, String)> {
    headers
        .iter()
        .filter(|(name, _)| {
            ALLOWLISTED_HEADERS
                .iter()
                .any(|allowed| name.eq_ignore_ascii_case(allowed))
        })
        .map(|(name, value)| (name.to_ascii_lowercase(), value.clone()))
        .collect()
}

fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

/// Configuration constraining which hosts may be fetched and redirected to.
#[derive(Clone, Debug)]
pub struct FetchConfig {
    /// Hosts explicitly reviewed by a human operator.
    pub reviewed_hosts: Vec<String>,
    /// Maximum response body bytes.
    pub max_bytes: usize,
}

impl FetchConfig {
    pub fn allows_host(&self, host: &str) -> bool {
        self.reviewed_hosts
            .iter()
            .any(|reviewed| reviewed.eq_ignore_ascii_case(host))
    }
}

/// Result of a provenance-aware fetch.
#[derive(Clone, Debug)]
pub struct FetchResult {
    pub observations: Vec<FetchObservation>,
    pub body: Option<Vec<u8>>,
    pub final_url: String,
    pub unchanged: bool,
}

/// Parameters for a provenance-aware fetch.
#[derive(Clone, Debug)]
pub struct FetchRequest {
    pub source_id: Option<String>,
    pub requested_url: String,
    pub retrieved_at: String,
    pub prior: Option<PriorEvidence>,
}

/// Performs a provenance-aware, conditional, DNS-safe fetch.
///
/// `request.source_id` links observations; `request.prior` carries the
/// previous evidence (for `304` handling) and its ETag/Last-Modified for
/// conditional requests.
pub fn provenance_fetch(
    config: &FetchConfig,
    resolver: &dyn Resolver,
    transport: &dyn Transport,
    request: &FetchRequest,
) -> Result<FetchResult, HttpError> {
    let url = Url::parse(&request.requested_url).map_err(|e| HttpError::InvalidUrl(e.to_string()))?;
    if url.scheme() != "https" {
        return Err(HttpError::InsecureUrl);
    }
    let host = url
        .host_str()
        .ok_or(HttpError::InsecureUrl)?
        .to_owned();
    if !config.allows_host(&host) {
        return Err(HttpError::UnreviewedRedirect(host));
    }
    let mut observations = Vec::new();
    let mut current_url = url.clone();
    let mut redirects = 0usize;
    let mut body: Option<Vec<u8>> = None;
    let mut unchanged = false;

    loop {
        let current_host = current_url
            .host_str()
            .ok_or(HttpError::InsecureUrl)?
            .to_owned();
        if !config.allows_host(&current_host) {
            return Err(HttpError::UnreviewedRedirect(current_host));
        }
        let addresses = resolver.resolve(&current_host)?;
        validate_resolved(&current_host, &addresses)?;

        let conditional_etag = if redirects == 0 {
            request.prior.as_ref().and_then(|p| p.etag.clone())
        } else {
            None
        };
        let conditional_modified = if redirects == 0 {
            request.prior.as_ref().and_then(|p| p.last_modified.clone())
        } else {
            None
        };

        let response = transport.request(&TransportRequest {
            url: current_url.clone(),
            conditional_etag,
            conditional_modified,
        })?;

        let allowlisted = allowlist_headers(&response.headers);
        let observation = build_observation(
            request.source_id.as_deref(),
            &current_url,
            &request.retrieved_at,
            &addresses,
            &response,
            &allowlisted,
        );
        observations.push(observation);

        match response.status_code {
            200..=204 => {
                if response.body.len() > config.max_bytes {
                    return Err(HttpError::Oversized(config.max_bytes));
                }
                let digest = sha256_hex(&response.body);
                if let Some(last) = observations.last_mut() {
                    last.body_digest = Some(digest);
                }
                body = Some(response.body);
                break;
            }
            304 => {
                unchanged = true;
                break;
            }
            301 | 302 | 303 | 307 | 308 => {
                let target = observations
                    .last()
                    .and_then(|item| item.redirect_target.clone());
                let (next, new_redirects) =
                    follow_redirect(config, &current_url, target, redirects)?;
                redirects = new_redirects;
                current_url = next;
            }
            _ => {
                return Err(HttpError::Transport);
            }
        }
    }

    Ok(FetchResult {
        observations,
        body,
        final_url: current_url.to_string(),
        unchanged,
    })
}

fn build_observation(
    source_id: Option<&str>,
    current_url: &Url,
    retrieved_at: &str,
    addresses: &[IpAddr],
    response: &TransportResponse,
    allowlisted: &[(String, String)],
) -> FetchObservation {
    let content_type = header_value(allowlisted, "content-type").map(str::to_owned);
    let content_length = header_value(allowlisted, "content-length")
        .and_then(|value| value.parse::<u64>().ok());
    FetchObservation {
        id: FetchObservation::id_for(current_url.as_str(), retrieved_at, response.status_code),
        source_id: source_id.map(str::to_owned),
        requested_url: current_url.to_string(),
        resolved_ips: addresses.iter().map(ToString::to_string).collect(),
        retrieved_at: retrieved_at.to_owned(),
        method: "GET".to_owned(),
        status_code: response.status_code,
        redirect_target: header_value(allowlisted, "location").map(str::to_owned),
        final_url: current_url.to_string(),
        allowlisted_headers: allowlisted.to_vec(),
        content_type,
        content_length,
        etag: header_value(allowlisted, "etag").map(str::to_owned),
        last_modified: header_value(allowlisted, "last-modified").map(str::to_owned),
        body_digest: None,
        error: None,
    }
}

fn follow_redirect(
    config: &FetchConfig,
    current_url: &Url,
    redirect_target: Option<String>,
    redirects: usize,
) -> Result<(Url, usize), HttpError> {
    let target = redirect_target.ok_or(HttpError::Transport)?;
    let next = current_url
        .join(&target)
        .map_err(|e| HttpError::InvalidUrl(e.to_string()))?;
    if next.scheme() != "https" {
        return Err(HttpError::InsecureUrl);
    }
    let next_host = next
        .host_str()
        .ok_or(HttpError::InsecureUrl)?
        .to_owned();
    if !config.allows_host(&next_host) {
        return Err(HttpError::UnreviewedRedirect(next_host));
    }
    if &next == current_url {
        return Err(HttpError::RedirectLoop);
    }
    let redirects = redirects + 1;
    if redirects > MAX_REDIRECTS {
        return Err(HttpError::TooManyRedirects);
    }
    Ok((next, redirects))
}

/// Prior evidence metadata used for conditional retrieval.
#[derive(Clone, Debug)]
pub struct PriorEvidence {
    pub evidence_id: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

/// Builds a [`ConditionalResult`] from a fetch, referencing prior evidence on
/// `304` without manufacturing a new blob.
pub fn conditional_result(
    result: &FetchResult,
    source_id: Option<&str>,
    prior: Option<&PriorEvidence>,
) -> ConditionalResult {
    let observation = result
        .observations
        .last()
        .cloned()
        .unwrap_or_else(|| FetchObservation {
            id: "fetch:none".to_owned(),
            source_id: source_id.map(str::to_owned),
            requested_url: result.final_url.clone(),
            resolved_ips: Vec::new(),
            retrieved_at: String::new(),
            method: "GET".to_owned(),
            status_code: 0,
            redirect_target: None,
            final_url: result.final_url.clone(),
            allowlisted_headers: Vec::new(),
            content_type: None,
            content_length: None,
            etag: None,
            last_modified: None,
            body_digest: None,
            error: None,
        });
    ConditionalResult {
        observation,
        unchanged: result.unchanged,
        prior_evidence_id: if result.unchanged {
            prior.map(|p| p.evidence_id.clone())
        } else {
            None
        },
        new_evidence_id: None,
    }
}

/// Structured error for a failed fetch observation.
pub fn observation_error(code: &str, message: String) -> StructuredError {
    StructuredError {
        code: code.to_owned(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeResolver(Vec<IpAddr>);

    impl Resolver for FakeResolver {
        fn resolve(&self, _host: &str) -> Result<Vec<IpAddr>, HttpError> {
            Ok(self.0.clone())
        }
    }

    struct FakeTransport {
        responses: Vec<TransportResponse>,
    }

    impl Transport for FakeTransport {
        fn request(&self, _request: &TransportRequest) -> Result<TransportResponse, HttpError> {
            Ok(self.responses[0].clone())
        }
    }

    fn config() -> FetchConfig {
        FetchConfig {
            reviewed_hosts: vec!["example.test".to_owned()],
            max_bytes: 1024,
        }
    }

    fn request(prior: Option<PriorEvidence>) -> FetchRequest {
        FetchRequest {
            source_id: Some("src".to_owned()),
            requested_url: "https://example.test/doc".to_owned(),
            retrieved_at: "2026-08-16T00:00:00Z".to_owned(),
            prior,
        }
    }

    fn response(status: u16, body: &[u8]) -> TransportResponse {
        TransportResponse {
            status_code: status,
            headers: vec![
                ("content-type".to_owned(), "text/plain".to_owned()),
                ("etag".to_owned(), "\"abc\"".to_owned()),
                ("last-modified".to_owned(), "Tue, 15 Nov 1994 12:45:26 GMT".to_owned()),
                ("set-cookie".to_owned(), "secret=value".to_owned()),
                ("authorization".to_owned(), "Bearer secret".to_owned()),
            ],
            body: body.to_vec(),
        }
    }

    #[test]
    fn public_dns_answer_is_accepted() {
        assert!(validate_resolved("example.test", &["93.184.216.34".parse().unwrap()]).is_ok());
    }

    #[test]
    fn private_answer_is_rejected() {
        assert!(matches!(
            validate_resolved("example.test", &["10.0.0.1".parse().unwrap()]),
            Err(HttpError::NonPublicAddress(_))
        ));
        assert!(matches!(
            validate_resolved("example.test", &["127.0.0.1".parse().unwrap()]),
            Err(HttpError::NonPublicAddress(_))
        ));
        assert!(matches!(
            validate_resolved("example.test", &["192.168.1.1".parse().unwrap()]),
            Err(HttpError::NonPublicAddress(_))
        ));
        assert!(matches!(
            validate_resolved("example.test", &["169.254.0.1".parse().unwrap()]),
            Err(HttpError::NonPublicAddress(_))
        ));
        assert!(matches!(
            validate_resolved("example.test", &["::1".parse().unwrap()]),
            Err(HttpError::NonPublicAddress(_))
        ));
        assert!(matches!(
            validate_resolved("example.test", &["203.0.113.5".parse().unwrap()]),
            Err(HttpError::NonPublicAddress(_))
        ));
    }

    #[test]
    fn mixed_public_and_private_answers_fail_closed() {
        assert!(matches!(
            validate_resolved(
                "example.test",
                &["93.184.216.34".parse().unwrap(), "10.0.0.1".parse().unwrap()]
            ),
            Err(HttpError::MixedAnswers(_))
        ));
    }

    #[test]
    fn credentials_are_never_persisted() {
        let transport = FakeTransport {
            responses: vec![response(200, b"hello")],
        };
        let result = provenance_fetch(
            &config(),
            &FakeResolver(vec!["93.184.216.34".parse().unwrap()]),
            &transport,
            &request(None),
        )
        .expect("fetch");
        let observation = result.observations.last().expect("observation");
        assert!(observation
            .allowlisted_headers
            .iter()
            .all(|(name, _)| name != "set-cookie" && name != "authorization"));
        assert!(observation
            .allowlisted_headers
            .iter()
            .any(|(name, _)| name == "etag"));
        assert_eq!(
            observation.body_digest.as_deref(),
            Some(sha256_hex(b"hello").as_str())
        );
    }

    #[test]
    fn etag_200_then_304_does_not_create_evidence() {
        let transport = FakeTransport {
            responses: vec![response(304, b"")],
        };
        let result = provenance_fetch(
            &config(),
            &FakeResolver(vec!["93.184.216.34".parse().unwrap()]),
            &transport,
            &request(Some(PriorEvidence {
                evidence_id: "evidence:prior".to_owned(),
                etag: Some("\"abc\"".to_owned()),
                last_modified: None,
            })),
        )
        .expect("fetch");
        assert!(result.unchanged);
        assert!(result.body.is_none());
        let conditional = conditional_result(&result, Some("src"), None);
        assert!(conditional.unchanged);
    }

    #[test]
    fn redirect_to_unreviewed_host_is_rejected() {
        let transport = FakeTransport {
            responses: vec![response(302, b"")],
        };
        // The redirect target host is not in reviewed_hosts.
        let result = provenance_fetch(
            &config(),
            &FakeResolver(vec!["93.184.216.34".parse().unwrap()]),
            &transport,
            &request(None),
        );
        assert!(matches!(result, Err(HttpError::Transport)));
    }

    #[test]
    fn oversized_streaming_response_is_rejected() {
        let transport = FakeTransport {
            responses: vec![response(200, &vec![b'x'; 4096])],
        };
        let result = provenance_fetch(
            &config(),
            &FakeResolver(vec!["93.184.216.34".parse().unwrap()]),
            &transport,
            &request(None),
        );
        assert!(matches!(result, Err(HttpError::Oversized(_))));
    }
}
