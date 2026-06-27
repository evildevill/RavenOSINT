use std::time::Duration;

use reqwest::{Client, ClientBuilder, Proxy, RequestBuilder};
use tracing::warn;

use crate::error::RavenError;
use crate::types::{interpolate_payload, ProbeMethod, SiteInfo};

pub fn create_http_client(
    timeout_secs: f64,
    proxy_url: Option<&str>,
) -> Result<Client, RavenError> {
    let mut builder = ClientBuilder::new()
        .timeout(Duration::from_secs_f64(timeout_secs.max(1.0)))
        .connect_timeout(Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:129.0) Gecko/20100101 Firefox/129.0")
        .tcp_nodelay(true)
        .pool_max_idle_per_host(100)
        .tcp_keepalive(Duration::from_secs(30));

    if let Some(proxy) = proxy_url {
        match Proxy::all(proxy) {
            Ok(p) => {
                builder = builder.proxy(p);
            }
            Err(e) => {
                warn!("Invalid proxy URL '{proxy}': {e}. Continuing without proxy.");
            }
        }
    }

    builder
        .build()
        .map_err(RavenError::Network)
}

pub fn build_site_request(
    client: &Client,
    site: &SiteInfo,
    url: &str,
    username: &str,
) -> RequestBuilder {
    let method = site.request_method.as_ref().unwrap_or(&ProbeMethod::Get);

    let mut req = match method {
        ProbeMethod::Get => client.get(url),
        ProbeMethod::Head => client.head(url),
        ProbeMethod::Post => client.post(url),
        ProbeMethod::Put => client.put(url),
    };

    if let Some(headers) = &site.headers {
        for (key, value) in headers {
            req = req.header(key.as_str(), value.as_str());
        }
    }

    if let Some(payload) = &site.request_payload {
        let interpolated = interpolate_payload(payload, username);
        req = req.json(&interpolated);
    }

    req
}
