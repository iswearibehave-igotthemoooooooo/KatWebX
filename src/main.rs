#![cfg_attr(feature = "cargo-clippy", warn(pedantic))]
#![cfg_attr(feature = "cargo-clippy", deny(cargo))]
#![cfg_attr(feature = "cargo-clippy", deny(all))]
#![cfg_attr(feature = "cargo-clippy", allow(multiple_crate_versions))]
#![cfg_attr(feature = "cargo-clippy", allow(borrow_interior_mutable_const))]

#[macro_use]
extern crate lazy_static;
extern crate futures;
extern crate actix;
extern crate actix_web;
extern crate rustls;
extern crate mime;
extern crate mime_guess;
extern crate mime_sniffer;
extern crate json;
extern crate regex;
extern crate bytes;
extern crate base64;
mod stream;
mod ui;
mod config;
use config::Config;
use actix_web::{actix::Actor, server, server::{RustlsAcceptor, ServerFlags}, client, client::ClientConnector, App, http::{header, header::{HeaderValue, HeaderMap}, Method, ContentEncoding, StatusCode}, HttpRequest, HttpResponse, HttpMessage, AsyncResponder, Error, dev::Payload};
use futures::future::{Future, result};
use std::{process, cmp, fs, string::String, fs::File, path::Path, io::Read, io::BufReader, time::Duration};
use base64::decode;
use mime_sniffer::MimeTypeSniffer;
use regex::{Regex, NoExpand};
use rustls::{NoClientAuth, ServerConfig, internal::pemfile::{certs, rsa_private_keys}};

lazy_static! {
	static ref conf: Config = Config::load_config("conf.json".to_owned(), true);
}

// Generate the correct host and path, from raw data.
// Hidden hosts can be virtual-host based (hidden.local) or regex-based.
// Redirects can be either full path based (localhost/redir) or regex-based.
// Reverse proxying can be either virtual-host based (proxy.local) or regex-based.
fn handle_path(path: &str, host: &str, auth: &str, c: &Config) -> (String, String, Option<String>) {
	let mut host = trim_port(host);
	let hostn = host.to_owned();
	let auth = &decode(trim_prefix("Basic ", auth)).unwrap_or_else(|_| vec![]);
	let auth = &*String::from_utf8_lossy(auth);

	let fp = &[host, path].concat();
	match path {
		_ if path.ends_with("/index.html") => return ("./".to_owned(), "redir".to_owned(), None),
		_ if path.contains("..") => return ("..".to_owned(), "redir".to_owned(), None),
		_ => (),
	}

	if c.authx.is_match(fp) {
		let mut r = "$x";
		if let Some(regx) = c.authx.matches(fp).iter().next() {r = &c.lauthx[regx]};
		if let Some(eauth) = c.authmap.get(&["r#", r].concat()) {
			if auth != eauth {
				return ("unauth".to_owned(), "redir".to_owned(), None)
			}
		}
	}

	match host {
		_ if c.redirx.is_match(fp) => {
			let mut r = "$x";
			if let Some(regx) = c.redirx.matches(fp).iter().next() {r = &c.lredirx[regx]}
			if let Some(link) = conf.redirmap.get(&["r#", r].concat()) {return ([link.to_owned(), trim_regex(r, &fp)].concat(), "redir".to_owned(), None)}
		},
		_ if c.lredir.binary_search(fp).is_ok() => {
			if let Some(link) = c.redirmap.get(fp) {return (link.to_owned(), "redir".to_owned(), None)}
		},
		_ if c.proxyx.is_match(fp) => {
			let mut r = "$x";
			if let Some(regx) = c.proxyx.matches(fp).iter().next() {r = &c.lproxyx[regx]}
			if let Some(link) = c.proxymap.get(&["r#", r].concat()) {return ([link.to_owned(), trim_regex(r, &fp)].concat(), "proxy".to_owned(), None)}
		},
		_ if c.lproxy.binary_search(&hostn).is_ok() => {
			if let Some(link) = c.proxymap.get(host) {return ([link, path].concat(), "proxy".to_owned(), None)}
		},
		_ if c.hidden.binary_search(&hostn).is_ok() => host = "html",
		_ if c.hiddenx.is_match(&hostn) => host = "html",
		_ if host.is_empty() || &host[..1] == "." || host.contains('/') || host.contains('\\') => host = "html",
		_ if !Path::new(&hostn).exists() => host = "html",
		_ => (),
	};

	let pathn;
	if path.ends_with('/') {
		pathn = [path, "index.html"].concat()
	} else {
		pathn = path.to_owned()
	}
	let full_path = [host, &*pathn].concat();

	(pathn, host.to_owned(), Some(full_path))
}

// Reverse proxy a request, passing through any compression.
// This cannot proxy websockets, because it removes the "Connection" HTTP header.
// Hop-by-hop headers are removed, to allow connection reuse.
fn proxy_request(path: &str, method: Method, headers: &HeaderMap, body: Payload, client_ip: &str, timeout: u64) -> Box<Future<Item=HttpResponse, Error=Error>> {
	let re = client::ClientRequest::build()
		.with_connector(
			ClientConnector::default()
				.conn_lifetime(Duration::from_secs(timeout*4))
				.conn_keep_alive(Duration::from_secs(timeout*4))
				.start().clone())
		.uri(path).method(method).disable_decompress()
		.if_true(true, |req| {
			for (key, value) in headers.iter() {
				match key.as_str() {
					"Connection" | "Proxy-Connection" | "Keep-Alive" | "Proxy-Authenticate" | "Proxy-Authorization" | "Te" | "Trailer" | "Transfer-Encoding" | "Upgrade" => (),
					"X-Forwarded-For" => {
						req.header("X-Forwarded-For", [value.to_str().unwrap_or("127.0.0.1"), ", ", client_ip].concat());
						continue
					},
					_ => {
						req.header(key.to_owned(), value.to_owned());
						continue
					},
				};
			}
		})
		.set_header_if_none("X-Forwarded-For", client_ip)
		.set_header_if_none(header::ACCEPT_ENCODING, "none")
		.set_header_if_none(header::USER_AGENT, "KatWebX-Proxy")
		.streaming(body);

	let req;
	match re {
		Ok(r) => req = r,
		Err(_) => return ui::http_error(StatusCode::BAD_GATEWAY, "502 Bad Gateway", "The server was acting as a proxy and received an invalid response from the upstream server."),
	}

	req.send().and_then(|resp| {
			Ok(HttpResponse::Ok()
				.status(resp.status())
				.if_true(true, |req| {
					for (key, value) in resp.headers().iter() {
						if key == header::CONTENT_LENGTH {
							continue
						}
						if key == header::CONTENT_ENCODING {
							// We don't want the data to be compressed more than once.
							req.content_encoding(ContentEncoding::Identity);
						}
						req.header(key.to_owned(), value.to_owned());
					}

					if let Ok(c) = resp.cookies() {{for ck in &c {
						req.cookie(ck.to_owned());
					}}}
				})
				.streaming(resp.payload()))
		}).or_else(|_| {
			ui::http_error(StatusCode::BAD_GATEWAY, "502 Bad Gateway", "The server was acting as a proxy and received an invalid response from the upstream server.")
		}).responder()
}

// Trim the port from an IPv4 address, IPv6 address, or domain:port.
fn trim_port(path: &str) -> &str {
	if path.contains('[') && path.contains(']') {
		match path.rfind("]:") {
			Some(i) => return &path[..=i],
			None => return path,
		};
	}

	match path.rfind(':') {
		Some(i) => &path[..i],
		None => path,
	}
}

// Trim the host from an IPv4 address, IPv6 address, or domain:port.
fn trim_host(path: &str) -> &str {
	if path.contains('[') && path.contains(']') {
		match path.rfind("]:") {
			Some(i) => return &path[i+1..],
			None => return "",
		};
	}

	match path.rfind(':') {
		Some(i) => &path[i..],
		None => "",
	}
}

// Trim a substring (prefix) from the beginning of a string.
fn trim_prefix<'a>(prefix: &'a str, root: &'a str) -> &'a str {
	match root.find(prefix) {
		Some(i) => &root[i+prefix.len()..],
		None => root,
	}
}

// Trim a substring (suffix) from the end of a string.
fn trim_suffix<'a>(suffix: &'a str, root: &'a str) -> &'a str {
	match root.rfind(suffix) {
		Some(i) => &root[..i],
		None => root,
	}
}

// Use regex to trim a string.
fn trim_regex(regex: &str, root: &str) -> String {
	let r = Regex::new(regex).unwrap_or_else(|_| Regex::new("$x").unwrap());
	r.replace_all(&root, NoExpand("")).to_string()
}

// Open both a file, and the file's metadata.
fn open_meta(path: &str) -> Result<(fs::File, fs::Metadata), Error> {
	let f = File::open(path)?;
	let m =  f.metadata()?;
	Ok((f, m))
}

// Do a HTTP permanent redirect.
fn redir(path: &str) -> Box<Future<Item=HttpResponse, Error=Error>> {
	result(Ok(
		HttpResponse::Ok()
			.status(StatusCode::PERMANENT_REDIRECT)
			.content_encoding(ContentEncoding::Auto)
			.header(header::LOCATION, path)
			.header(header::SERVER, "KatWebX")
			.content_type("text/html; charset=utf-8")
			.body(["<a href='", path, "'>If this redirect does not work, click here</a>"].concat())))
			.responder()
}

// Return a MIME type based on file extension.
// If the file extension is not known, attempt to guess the mime type.
fn get_mime(path: &str) -> String {
	let mut mime = mime_guess::guess_mime_type(path).to_string();
	if mime == "application/octet-stream" {
		let (mut f, finfo);
		match open_meta(path) {
			Ok((fi, m)) => {f = fi; finfo = m},
			Err(_) => {
				return mime
			}
		}

		let mut sniffer_data = vec![0; cmp::min(512, finfo.len() as usize)];
		f.read_exact(&mut sniffer_data).unwrap_or(());

		let mreq = mime_sniffer::HttpRequest {
			content: &sniffer_data,
			url: &["http://localhost", path].concat(),
			type_hint: "",
		};

		mime = mreq.sniff_mime_type().unwrap_or("").to_owned();
	}
	if mime.starts_with("text/") && !mime.contains("charset") {
		return [&mime, "; charset=utf-8"].concat();
	}

	mime
}

// HTTP(S) request handling.
fn index(req: &HttpRequest) -> Box<Future<Item=HttpResponse, Error=Error>> {
	let conn_info = req.connection_info();

	if conf.hsts && conn_info.scheme() == "http" {
		let host = trim_port(conn_info.host());
		let tls_host = conf.tls_addr.to_owned();
		if trim_host(&tls_host) == ":443" {
			return redir(&["https://", &host, req.path()].concat());
		}
		return redir(&["https://", &host, trim_host(&tls_host), req.path()].concat());
	}

	let blankhead = &HeaderValue::from_static("");
	let (path, host, fp) = handle_path(req.path(), conn_info.host(), req.headers().get(header::AUTHORIZATION).unwrap_or(blankhead).to_str().unwrap_or(""), &conf);
	//println!("{:?}", [&trim_port(conn_info.host().to_owned()), req.path()].concat());

	if host == "redir" {
		if path == "unauth" {
			return ui::http_error(StatusCode::UNAUTHORIZED, "401 Unauthorized", "Valid credentials are required to acccess this resource.")
		}
		return redir(&path);
	}

	if host == "proxy" {
		return proxy_request(&path, req.method().to_owned(), req.headers(), req.payload(), conn_info.remote().unwrap_or("127.0.0.1"), conf.stream_timeout)
	}

	if req.method() != Method::GET && req.method() != Method::HEAD {
		return ui::http_error(StatusCode::METHOD_NOT_ALLOWED, "405 Method Not Allowed", "Only GET and HEAD methods are supported.")
	}

	let mut full_path = match fp {
		Some(pf) => pf,
		None => [&*host, &*path].concat(),
	};

	let mime = get_mime(&full_path);
	let mim = trim_suffix("; charset=utf-8", &mime);

	// If the client accepts a brotli compressed response, then modify full_path to send one.
	let ce = req.headers().get(header::ACCEPT_ENCODING).unwrap_or(blankhead).to_str().unwrap_or("");
	if ce.contains("br") {
		if conf.compress_files {
			if let Ok(path) = stream::get_compressed_file(&&*full_path, mim) {full_path = path}
		} else if Path::new(&[&full_path, ".br"].concat()).exists() {
			full_path = [&full_path, ".br"].concat()
		}
	}

	// Open the file specified in full_path. If the file is not present, serve either a directory listing or an error.
	let (f, finfo);
	if let Ok((fi, m)) = open_meta(&full_path) {f = fi; finfo = m} else {
		if path.ends_with("/index.html") {
			return ui::dir_listing(&[&*host, req.path()].concat(), &host)
		}

		return ui::http_error(StatusCode::NOT_FOUND, "404 Not Found", &["The resource ", req.path(), " could not be found."].concat());
	}

	if finfo.is_dir() {
		return redir(&[req.path(), "/"].concat());
	}

	// Parse a ranges header if it is present, and then turn a File into a stream.
	let (length, offset) = stream::calculate_ranges(&req.drop_state(), finfo.len());
	let reader = stream::ChunkedReadFile {
		offset,
		size: length,
		cpu_pool: req.cpu_pool().clone(),
		file: Some(f),
		fut: None,
		counter: 0,
	};

	// Craft a response.
	let cache_int = conf.caching_timeout;
	result(Ok(
		HttpResponse::Ok()
	        .content_type(&*mime)
			.header(header::ACCEPT_RANGES, "bytes")
			.if_true(full_path.ends_with(".br"), |builder| {
				builder.header(header::CONTENT_ENCODING, "br");
				builder.content_encoding(ContentEncoding::Identity);
			})
			.if_true(!full_path.ends_with(".br") && stream::gztypes.binary_search(&&*mime).is_ok(), |builder| {
				builder.content_encoding(ContentEncoding::Auto);
			})
			.if_true(offset != 0, |builder| {
				builder.header(header::CONTENT_RANGE, ["bytes ", &offset.to_string(), "-", &(offset+length-1).to_string(), "/", &finfo.len().to_string()].concat());
			})
			.if_true(cache_int == 0, |builder| {
				builder.header(header::CACHE_CONTROL, "no-store, must-revalidate");
			})
			.if_true(cache_int != 0, |builder| {
				builder.header(header::CACHE_CONTROL, ["max-age=", &(cache_int*3600).to_string(), ", public, stale-while-revalidate=", &(cache_int*900).to_string()].concat());
			})
			.if_true(conf.hsts, |builder| {
				builder.header(header::STRICT_TRANSPORT_SECURITY, "max-age=31536000;includeSubDomains;preload");
			})
			.if_true(conf.protect, |builder| {
				builder.header(header::REFERRER_POLICY, "no-referrer");
				builder.header(header::X_CONTENT_TYPE_OPTIONS, "nosniff");
				builder.header(header::CONTENT_SECURITY_POLICY, "default-src https: data: 'unsafe-inline' 'unsafe-eval' 'self'; frame-ancestors 'self'");
				builder.header(header::X_XSS_PROTECTION, "1; mode=block");
			})
			.header(header::SERVER, "KatWebX-Alpha")
            .streaming(reader)))
        	.responder()
}

// Load configuration, SSL certs, then attempt to start the program.
fn main() {
	println!("[Info]: Starting KatWebX...");
	let sys = actix::System::new("katwebx");
	lazy_static::initialize(&conf);

	let mut tconfig = ServerConfig::new(NoClientAuth::new());
	let cert_file = &mut BufReader::new(File::open("ssl/cert.pem").unwrap_or_else(|_err| {
		println!("[Fatal]: Unable to load ssl/cert.pem!");
		process::exit(1);
	}));
	let key_file = &mut BufReader::new(File::open("ssl/key.pem").unwrap_or_else(|_err| {
		println!("[Fatal]: Unable to load ssl/key.pem!");
		process::exit(1);
	}));
	let cert_chain = certs(cert_file).unwrap_or_else(|_err| {
		println!("[Fatal]: Unable to parse tls certificates!");
		process::exit(1);
	});
	let mut keys = rsa_private_keys(key_file).unwrap_or_else(|_err| {
		println!("[Fatal]: Unable to parse private key!");
		process::exit(1);
	});
	tconfig.set_single_cert(cert_chain, keys.remove(0)).unwrap_or_else(|_err| {
		println!("[Fatal]: Unable to parse private key!");
		process::exit(1);
	});
	let acceptor = RustlsAcceptor::with_flags(
		tconfig,
		ServerFlags::HTTP1 | ServerFlags::HTTP2,
	);

	// Request handling
    server::new(|| {
		App::new()
			.default_resource(|r| r.f(index))
	})
		.backlog(8192).maxconn(100_000).maxconnrate(4096)
		.keep_alive(conf.stream_timeout as usize)
		.bind_with(&conf.tls_addr, move || acceptor.clone())
		.unwrap_or_else(|_err| {
			println!("{}", ["[Fatal]: Unable to bind to ", &conf.tls_addr, "!"].concat());
			process::exit(1);
		})
		.bind(&conf.http_addr)
		.unwrap_or_else(|_err| {
			println!("{}", ["[Fatal]: Unable to bind to ", &conf.http_addr, "!"].concat());
			process::exit(1);
		})
        .start();

	println!("[Info]: Started KatWebX.");
	let _ = sys.run();
	println!("\n[Info]: Stopping KatWebX...");
}

// Unit tests for critical internal functions.
#[cfg(test)]
mod tests {
	use {config, handle_path, trim_port, trim_host, trim_prefix, trim_suffix};
	fn default_conf() -> config::Config {
		return config::Config::load_config(config::DEFAULT_CONFIG.to_owned(), false);
	}
	#[test]
	fn test_conf_defaults() {
		let conf = default_conf();
		assert_eq!(conf.caching_timeout, 4);
		assert_eq!(conf.stream_timeout, 20);
		assert_eq!(conf.hsts, false);
		assert_eq!(conf.protect, true);
	}
	#[test]
	fn test_trim_port() {
		assert_eq!(trim_port("127.0.0.1:8080"), "127.0.0.1");
		assert_eq!(trim_port("127.0.0.1"), "127.0.0.1");
		assert_eq!(trim_port("[::1]:8081"), "[::1]");
		assert_eq!(trim_port("[::1]"), "[::1]");
	}
	#[test]
	fn test_trim_host() {
		assert_eq!(trim_host("127.0.0.1:8080"), ":8080");
		assert_eq!(trim_host("127.0.0.1"), "");
		assert_eq!(trim_host("[::1]:8081"), ":8081");
		assert_eq!(trim_host("[::1]"), "");
	}
	#[test]
	fn test_trim_prefix() {
		assert_eq!(trim_prefix("str", "string"), "ing");
		assert_eq!(trim_prefix("no", "string"), "string");
		assert_eq!(trim_prefix("ing", "string"), "");
	}
	#[test]
	fn test_trim_suffix() {
		assert_eq!(trim_suffix("ing", "string"), "str");
		assert_eq!(trim_suffix("no", "string"), "string");
		assert_eq!(trim_suffix("str", "string"), "");
	}
	#[test]
	fn test_handle_path_base() {
		let conf = default_conf();
		assert_eq!(handle_path("/index.html", "localhost", "", &conf), ("./".to_owned(), "redir".to_owned(), None));
		assert_eq!(handle_path("/test/..", "localhost", "", &conf), ("..".to_owned(), "redir".to_owned(), None));

		assert_eq!(handle_path("/", "H", "", &conf), ("/index.html".to_owned(), "html".to_owned(), Some("html/index.html".to_owned())));
		assert_eq!(handle_path("/", "...", "", &conf), ("/index.html".to_owned(), "html".to_owned(), Some("html/index.html".to_owned())));
		assert_eq!(handle_path("/", "/home", "", &conf), ("/index.html".to_owned(), "html".to_owned(), Some("html/index.html".to_owned())));
		assert_eq!(handle_path("/", "C:\\", "", &conf), ("/index.html".to_owned(), "html".to_owned(), Some("html/index.html".to_owned())));

		assert_eq!(handle_path("/", "ssl", "", &conf), ("/index.html".to_owned(), "html".to_owned(), Some("html/index.html".to_owned())));
		assert_eq!(handle_path("/", "nonexistenthost", "", &conf), ("/index.html".to_owned(), "html".to_owned(), Some("html/index.html".to_owned())));
	}
	#[test]
	fn test_handle_path_routing() {
		let conf = default_conf();
		assert_eq!(handle_path("/redir", "localhost", "", &conf), ("https://kittyhacker101.tk".to_owned(), "redir".to_owned(), None));
		assert_eq!(handle_path("/redir2a", "localhost", "", &conf), ("https://google.com".to_owned(), "redir".to_owned(), None));

		assert_eq!(handle_path("/links.html", "proxy.local", "", &conf), ("https://kittyhacker101.tk/links.html".to_owned(), "proxy".to_owned(), None));
		assert_eq!(handle_path("/proxy0/links.html", "localhost", "", &conf), ("https://kittyhacker101.tk/links.html".to_owned(), "proxy".to_owned(), None));

		assert_eq!(handle_path("/", "src", "", &conf), ("/index.html".to_owned(), "html".to_owned(), Some("html/index.html".to_owned())));
		assert_eq!(handle_path("/", "target", "", &conf), ("/index.html".to_owned(), "html".to_owned(), Some("html/index.html".to_owned())));
		assert_eq!(handle_path("/", "html", "", &conf), ("/index.html".to_owned(), "html".to_owned(), Some("html/index.html".to_owned())));

		assert_eq!(handle_path("/", "html", "", &conf), ("/index.html".to_owned(), "html".to_owned(), Some("html/index.html".to_owned())));

		assert_eq!(handle_path("/demopass/", "localhost", "", &conf), ("unauth".to_owned(), "redir".to_owned(), None));
		assert_eq!(handle_path("/demopass/", "localhost", "aW5jb3JyZWN0OmxvZ2lu", &conf), ("unauth".to_owned(), "redir".to_owned(), None));
		assert_eq!(handle_path("/demopass/", "localhost", "YWRtaW46cGFzc3dk", &conf), ("/demopass/index.html".to_owned(), "html".to_owned(), Some("html/demopass/index.html".to_owned())));
	}
}
