// Ui.rs is responsible for creating all server-generated content (error pages, file listings, etc...)

// This is currently a non-issue, and can be ignored.
#![allow(clippy::cast_precision_loss)]

extern crate actix_web;
extern crate htmlescape;
extern crate number_prefix;
extern crate glob;
use actix_web::{HttpResponse, http::{header, ContentEncoding, StatusCode}, middleware::BodyEncoding};
use std::borrow::Borrow;
use self::htmlescape::{encode_minimal, encode_attribute};
use self::number_prefix::{NumberPrefix, Standalone, Prefixed, PrefixNames};

// dir_listing generates a webpage that lists the contents of a directory.
pub fn dir_listing(path: &str, trim: &str) -> HttpResponse {
	let f;

	match glob::glob(&[path, "/*"].concat()) {
		Ok(fi) => {f = fi},
		Err(_) => {
			return http_error(StatusCode::NOT_FOUND, "404 Not Found", &["The resource ", path, " could not be found."].concat(), false)
		}
	}

	let html = [HEAD, "<title>Directory listing of ", &encode_minimal(&path[trim.len()..]), "</title><h1 class=ok>", FOLDERSVG, "Directory listing of ", &encode_minimal(&path[trim.len()..]), "</h1><table><tr><td><span>Name</span></td><td><span>Size</span></td></tr><tr><td><a href='..'>", BACKSVG, "Back</a></td></tr>"].concat();
	let (mut html1, mut html2) = ("".to_owned(), "".to_owned());
	for fpath in f {
		let fstr = fpath.unwrap();
		let (name, size, icon);
		match fstr.file_name() {
			Some(fst) => {name = fst.to_string_lossy()},
			None => {
				return http_error(StatusCode::NOT_FOUND, "500 Internal Server Error", "An unexpected condition was encountered.", false)
			}
		}
		let mut namep = fstr.to_string_lossy()[trim.len()..].to_owned();
		if let Ok(fmeta) = fstr.metadata() {
			size = fmeta.len();
			if fmeta.is_dir() {
				icon = FOLDERSVG;
				namep.push_str("/")
			} else {
				icon = FILESVG
			}
		} else {
			size = 0;
			icon = FILESVG
		}

		let mut sizestr = "".to_owned();
		if icon != FOLDERSVG {
			match NumberPrefix::decimal(size as f64) {
				Standalone(bytes)   => {sizestr = [bytes.to_string(), "b".to_owned()].concat()}
				Prefixed(prefix, n) => {sizestr = [&((n*10_f64).round()/10_f64).to_string(), prefix.symbol()].concat()}
			}
			html2 = [&html2, "<tr><td><a href='", &encode_attribute(&namep), "'>", icon, &encode_minimal(name.borrow()), "</td><td><span>", &sizestr, "</span></a></td></tr>"].concat();
			continue
		}

		html1 = [&html1, "<tr><td><a href='", &encode_attribute(&namep), "'>", icon, &encode_minimal(name.borrow()), "</td><td><span>", &sizestr, "</span></a></td></tr>"].concat();
	}

	// Return file listing page
	let html = [html, html1, html2, "</table><span class=btmright>Powered by KatWebX</span>".to_owned()].concat();
	HttpResponse::Ok()
		.encoding(ContentEncoding::Auto)
		.header(header::SERVER, "KatWebX")
		.content_type("text/html; charset=utf-8")
		.body(html)
}

// http_error generates a server HTTP error page, using the provided data.
// If the "smaller" option is set to true, the page will be generated using the default sans-serif font, instead of the Product Sans font.
pub fn http_error(status: StatusCode, header: &str, body: &str, smaller: bool) -> HttpResponse {
	let head = if smaller {HEADSPL} else {HEAD};

	HttpResponse::Ok()
		.status(status)
		.encoding(ContentEncoding::Auto)
		.if_true(status == StatusCode::UNAUTHORIZED, |builder| {
			builder.header(header::WWW_AUTHENTICATE, "Basic realm=\"Please provide valid credentials to access this resource.\"");
		})
		.header(header::SERVER, "KatWebX")
		.content_type("text/html; charset=utf-8")
		.body([head, "<title>", header, "</title><h1 class=err>", ERRSVG, &encode_minimal(header), "</h1><p>", &encode_minimal(body), "</p><span class=bottom>Powered by KatWebX</span>"].concat())
}

// The head section of all HTML5 documents generated by the server. This contains a doctype, viewport, CSS, and a minimal woff2 font.
const HEAD: &str = r"<!DOCTYPE HTML><meta content='width=device-width,initial-scale=1,minimum-scale=1,maximum-scale=1' name=viewport><style>body *{margin:0;font:300 32px product-sans,sans-serif;color:#404040;text-decoration:none}h1{margin:.2em 0;font-size:60px}svg{height:60px;position:relative}h1 svg{top:8px;right:5px}a svg{height:32px;top:6px;right:6px}.ok{color:#20b48c;fill:#20b48c}.err{color:#b42020;fill:#b42020}span{color:#828282;font-size:24px}body{margin:30px 40px}.bottom{bottom:32px}th{text-align:left}td:nth-child(even){width:50px}td:nth-child(odd){width:400px}.bottom,.btmright{position:absolute}.btmright{padding:.3em 0}@media all and (max-width:600px){td:nth-child(even){display:none}}@media all and (min-width:900px){.btmright{position:fixed!important;bottom:32px;right:32px;padding:0!important}}@font-face{font-family:'product-sans';src:url(data:application/font-woff2;charset=utf-8;base64,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) format('woff2')}</style>";

// An alternative version of HEAD, with less CSS, and no included font. This will only work with the http_error function.
const HEADSPL: &str = r"<!DOCTYPE HTML><meta content='width=device-width,initial-scale=1,minimum-scale=1,maximum-scale=1' name=viewport><style>body *{margin:0;font:300 32px sans-serif;color:#404040}h1{margin:.2em 0;font-size:60px}svg{height:60px;position:relative;top:8px;right:5px}.err{color:#b42020;fill:#b42020}body{margin:30px 40px}.bottom{display:none}</style>";

// The icon used on server generated error pages.
const ERRSVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M11 15h2v2h-2zm0-8h2v6h-2zm1-5a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm0 18a8 8 0 1 1 0-16 8 8 0 0 1 0 16z" fill="#b42020"/></svg>"##;

// The back icon used on server generated file listings.
const BACKSVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M11 9l1 1-3 4h9V4h2v12H9l3 4-1 1-6-6 6-6z" fill="#404040"/></svg>"##;

// The file icon used on server generated file listings.
const FILESVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M13.5-4.4H-1.6A2.5 2.5 0 0 0-4.2-2v17.7h2.6V-2h15.1v-2.5zm-1.3 5l7.6 7.6v12.6c0 1.4-1.1 2.5-2.5 2.5H3.4a2.5 2.5 0 0 1-2.5-2.5V3.2C.9 1.8 2 .6 3.4.6h8.8zM11 9.5h7l-7-7v7z"/></svg>"##;

// The folder icon used on server generated file listings.
const FOLDERSVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" style="right: 8px"><path d="M20 6h-8l-2-2H4a2 2 0 0 0-2 2v12c0 1.1.9 2 2 2h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2zm0 12H4V8h16v10z"/></svg>"##;