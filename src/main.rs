#[macro_use]
extern crate lazy_static;
extern crate bytes;
extern crate futures;
extern crate actix_web;
extern crate openssl;
extern crate mime;
extern crate mime_guess;
extern crate mime_sniffer;
extern crate json;
mod stream;
mod ui;
use actix_web::{server, server::ServerFlags, App, HttpRequest, HttpResponse, AsyncResponder, Error, http::StatusCode, http::header, http::Method, server::OpensslAcceptor};
use openssl::ssl::{SslMethod, SslAcceptor, SslFiletype};
use futures::future::{Future, result};
use std::{process, cmp, fs, fs::File, path::Path, io::Read};
use mime_sniffer::MimeTypeSniffer;

fn open_meta(path: &str) -> Result<(fs::File, fs::Metadata), Error> {
	let f = File::open(path)?;
	let m =  f.metadata()?;
	return Ok((f, m));
}

fn redir(path: &str) -> Box<Future<Item=HttpResponse, Error=Error>> {
	return result(Ok(
		HttpResponse::Ok()
			.status(StatusCode::PERMANENT_REDIRECT)
			.header(header::LOCATION, path)
			.content_type("text/html; charset=utf-8")
			.body(["<a href='", path, "'>Click here</a>"].concat())))
			.responder();
}

fn get_mime(data: &Vec<u8>, path: &str) -> String {
	let mut mime = mime_guess::guess_mime_type(path).to_string();
	if mime == "application/octet-stream" {
		let mreq = mime_sniffer::HttpRequest {
			content: data,
			url: &["http://localhost", path].concat(),
			type_hint: "",
		};

		mime = mreq.sniff_mime_type().unwrap_or("").to_string();
	}
	if mime.starts_with("text/") && !mime.contains("charset") {
		return [mime, "; charset=utf-8".to_string()].concat();
	}

	return mime
}

fn sort_json(array: &json::Array) -> Vec<String> {
	let mut tmp = Vec::new();
	for item in array {
		tmp.push(item.as_str().unwrap_or("").to_string())
	}
	tmp.sort_unstable();
	return tmp
}

lazy_static! {
	static ref confraw: String = fs::read_to_string("conf.json").unwrap_or("{\"cachingTimeout\": 4,\"hide\": [\"src\"],\"advanced\": {\"protect\": true,\"httpAddr\": \"[::]:80\",\"tlsAddr\": \"[::]:443\"}}".to_string());
	static ref config: json::JsonValue<> = json::parse(&confraw).unwrap_or_else(|_err| {
		println!("[Fatal]: Unable to parse configuration!");
		process::exit(1);
	});
	static ref hidden: Vec<String> = match &config["hide"] {
		json::JsonValue::Array(array) => sort_json(array),
		_ => Vec::new(),
	};
}

fn index(_req: &HttpRequest) -> Box<Future<Item=HttpResponse, Error=Error>> {
	if _req.method() != Method::GET && _req.method() != Method::HEAD {
		return ui::http_error(StatusCode::METHOD_NOT_ALLOWED, "405 Method Not Allowed", "Only GET and HEAD methods are supported.")
	}

	let mut pathd = [_req.path()].concat();
	if pathd.ends_with("/") {
		pathd = [pathd, "index.html".to_string()].concat();
	} else if pathd.ends_with("/index.html") {
		return redir("./");
	}
	let path = &pathd;

	let conn_info = _req.connection_info();
	let mut host = conn_info.host();
	if host == "ssl" || host.len() < 1 || host[..1] == ".".to_string() || host.contains("/") || host.contains("\\") || hidden.binary_search(&host.to_string()).is_ok() {
		host = "html"
	}
	println!("{:?}",[host, path].concat());
	if !Path::new(host).exists() {
		host = "html"
	}

	if path.contains("..") {
		return ui::http_error(StatusCode::FORBIDDEN, "403 Forbidden", "You do not have permission to access this resource.")
	}

	let (mut f, finfo);

	match open_meta(&[host, path].concat()) {
		Ok((fi, m)) => {f = fi; finfo = m},
		Err(_) => {
			if path.ends_with("/index.html") {
				return ui::dir_listing(&[host, _req.path()].concat(), host)
			}

			return ui::http_error(StatusCode::NOT_FOUND, "404 Not Found", &["The resource ", _req.path(), " could not be found."].concat())
		}
	}

	if finfo.is_dir() {
		return redir(&[_req.path(), "/"].concat());
	}

	let mut sniffer_data = vec![0; cmp::min(512, finfo.len() as usize)];
	f.read_exact(&mut sniffer_data).unwrap_or(());

	let reader = stream::ChunkedReadFile {
		offset: 0,
		size: finfo.len(),
		cpu_pool: _req.cpu_pool().clone(),
		file: Some(f),
		fut: None,
		counter: 0,
	};

	let cache_int = config["cachingTimeout"].as_i64().unwrap_or(0);
	result(Ok(
		HttpResponse::Ok()
	        .content_type(get_mime(&sniffer_data, &[host, path].concat()))
			.if_true(cache_int == 0, |builder| {
				builder.header(header::CACHE_CONTROL, "no-store, must-revalidate");
			})
			.if_true(cache_int != 0, |builder| {
				builder.header(header::CACHE_CONTROL, ["max-age=".to_string(), (cache_int*3600).to_string(), ", public, stale-while-revalidate=".to_string(), (cache_int*900).to_string()].concat());
			})
			.if_true(config["advanced"]["protect"].as_bool().unwrap_or(false), |builder| {
				builder.header(header::REFERRER_POLICY, "no-referrer");
				builder.header(header::X_CONTENT_TYPE_OPTIONS, "nosniff");
				builder.header(header::CONTENT_SECURITY_POLICY, "default-src https: data: 'unsafe-inline' 'unsafe-eval' 'self'; frame-ancestors 'self'");
				builder.header(header::X_XSS_PROTECTION, "1; mode=block");
			})
			.header(header::SERVER, "KatWebX-Alpha")
            .streaming(reader)))
        	.responder()
}

fn main() {
	fs::write("conf.json", config.pretty(2)).unwrap_or_else(|_err| {
		println!("[Warn]: Unable to write configuration!");
	});

	let mut builder = SslAcceptor::mozilla_modern(SslMethod::tls()).unwrap_or_else(|_err| {
		println!("[Fatal]: Unable to create OpenSSL builder!");
		process::exit(1);
	});
	builder.set_private_key_file("ssl/key.pem", SslFiletype::PEM).unwrap_or_else(|_err| {
		println!("[Fatal]: Unable to load ssl/key.pem!");
		process::exit(1);
	});
	builder.set_certificate_chain_file("ssl/cert.pem").unwrap_or_else(|_err| {
		println!("[Fatal]: Unable to load ssl/cert.pem!");
		process::exit(1);
	});
	let acceptor = OpensslAcceptor::with_flags(builder, ServerFlags::HTTP1 | ServerFlags::HTTP2).unwrap_or_else(|_err| {
		println!("[Fatal]: Unable to create OpenSSL acceptor!");
		process::exit(1);
	});

    server::new(|| {
        vec![
			App::new()
				.default_resource(|r| r.f(index))
		]
	})
		.keep_alive(config["streamTimeout"].as_usize().unwrap_or(0)*4)
		.bind_with(config["advanced"]["tlsAddr"].as_str().unwrap_or("[::]:443"), acceptor)
		.unwrap_or_else(|_err| {
			println!("{}", ["[Fatal]: Unable to bind to ".to_string(), config["advanced"]["tlsAddr"].as_str().unwrap_or("[::]:443").to_string(), "!".to_string()].concat());
			process::exit(1);
		})
		.bind(config["advanced"]["httpAddr"].as_str().unwrap_or("[::]:80"))
		.unwrap_or_else(|_err| {
			println!("{}", ["[Fatal]: Unable to bind to ".to_string(), config["advanced"]["httpAddr"].as_str().unwrap_or("[::]:80").to_string(), "!".to_string()].concat());
			process::exit(1);
		})
        .run();
}
