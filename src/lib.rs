#![warn(rust_2018_idioms, unreachable_pub)]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[macro_use]
extern crate static_assertions;

use std::{collections::HashMap, time::Duration};

use hyper::{Body, Request};
use hyper::client::ResponseFuture;
use hyper::client::connect::HttpConnector;

pub use clickhouse_derive::Row;
#[cfg(feature = "tls")]
#[cfg_attr(docsrs, doc(cfg(feature = "tls")))]
use hyper_tls::HttpsConnector;

use self::error::{Error, Result};
pub use self::{compression::Compression, row::Row};

pub mod error;
pub mod insert;
pub mod inserter;
pub mod query;
pub mod sql;
#[cfg(feature = "test-util")]
#[cfg_attr(docsrs, doc(cfg(feature = "test-util")))]
pub mod test;
#[cfg(feature = "watch")]
#[cfg_attr(docsrs, doc(cfg(feature = "watch")))]
pub mod watch;

mod buflist;
mod compression;
mod cursor;
mod response;
mod row;
mod rowbinary;

mod sealed {
    pub trait Sealed {}
}

const TCP_KEEPALIVE: Duration = Duration::from_secs(60);

// ClickHouse uses 3s by default.
// See https://github.com/ClickHouse/ClickHouse/blob/368cb74b4d222dc5472a7f2177f6bb154ebae07a/programs/server/config.xml#L201
const POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub struct Client {
    plaintext_client: hyper::Client<HttpConnector>,
    #[cfg(feature = "tls")]
    tls_client: hyper::Client<HttpsConnector<HttpConnector>>,

    url: String,
    database: Option<String>,
    user: Option<String>,
    password: Option<String>,
    compression: Compression,
    options: HashMap<String, String>,
    headers: HashMap<String, String>,
}

impl Default for Client {
    fn default() -> Self {
        let mut plaintext_connector = HttpConnector::new();
        // TODO: make configurable in `Client::builder()`.
        plaintext_connector.set_keepalive(Some(TCP_KEEPALIVE));
        plaintext_connector.enforce_http(false);

        let plaintext_client = hyper::Client::builder()
            .pool_idle_timeout(POOL_IDLE_TIMEOUT)
            .build(plaintext_connector.clone());

        Self {
            plaintext_client,
            #[cfg(feature = "tls")]
            tls_client: {
                let tls_connector = HttpsConnector::new_with_connector(plaintext_connector);
                hyper::Client::builder()
                    .pool_idle_timeout(POOL_IDLE_TIMEOUT)
                    .build(tls_connector)
            },
            url: String::new(),
            database: None,
            user: None,
            password: None,
            compression: Compression::default(),
            options: HashMap::new(),
            headers: HashMap::new(),
        }
    }
}

impl Client {
    // TODO: use `url` crate?
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    pub fn with_database(mut self, database: impl Into<String>) -> Self {
        self.database = Some(database.into());
        self
    }

    pub fn with_user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    pub fn with_password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    pub fn with_compression(mut self, compression: Compression) -> Self {
        // TODO: remove when compression will be implemented.
        self.compression = if cfg!(feature = "test-util") {
            Compression::None
        } else {
            compression
        };
        self
    }

    pub fn with_option(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.options.insert(name.into(), value.into());
        self
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    /// Starts a new INSERT statement.
    ///
    /// # Panics
    /// If `T` has unnamed fields, e.g. tuples.
    pub fn insert<T: Row>(&self, table: &str) -> Result<insert::Insert<T>> {
        insert::Insert::new(self, table)
    }

    pub fn inserter<T: Row>(&self, table: &str) -> Result<inserter::Inserter<T>> {
        inserter::Inserter::new(self, table)
    }

    pub fn query(&self, query: &str) -> query::Query {
        query::Query::new(self, query)
    }

    fn request(&self, req: Request<Body>) -> Result<ResponseFuture> {
        match req.uri().scheme_str() {
            Some("http") => Ok(self.plaintext_client.request(req)),
            #[cfg(feature = "tls")]
            Some("https") => Ok(self.tls_client.request(req)),
            scheme => Err(Error::UnsupportedScheme(scheme.map(|x| x.to_owned()))),
        }
    }

    #[cfg(feature = "watch")]
    pub fn watch(&self, query: &str) -> watch::Watch {
        watch::Watch::new(self, query)
    }
}
