## KatWebX [![Build status](https://ci.appveyor.com/api/projects/status/9fjk67yk8ei7hnlg/branch/master?svg=true)](https://ci.appveyor.com/project/kittyhacker101/katwebx/branch/master) [![Build Status](https://travis-ci.com/kittyhacker101/KatWebX.svg?branch=master)](https://travis-ci.com/kittyhacker101/KatWebX) [![Percentage of issues still open](http://isitmaintained.com/badge/open/kittyhacker101/KatWebX.svg)](http://isitmaintained.com/project/kittyhacker101/KatWebX "Percentage of issues still open") [![Average time to resolve an issue](http://isitmaintained.com/badge/resolution/kittyhacker101/KatWebX.svg)](http://isitmaintained.com/project/kittyhacker101/KatWebX "Average time to resolve an issue")
An extremely fast static web-server and reverse proxy for the modern web. More info is available on [KatWebX.kittyhacker101.tk](https://katwebx.kittyhacker101.tk/).

## Important Info
KatWebX is stil a work in progress, and you may encounter issues. **KatWebX is not well tested, production use is not recommended!**  If you need something which will is well tested and can be used in production, check out [KatWeb](https://github.com/kittyhacker101/KatWeb) instead.

Interested in the project? You can help fund KatWebX's development by donating to the Bitcoin address `1KyggZGHF4BfHoHEXxoGzDmLmcGLaHN2x2`.

## Release Schedule
Approximate dates for the release of KatWebX (and discontinuing of KatWeb) are listed below.
- March 17 - KatWebX's first release.
- March 24 - A tool is released to automatically migrate existing setups from KatWeb to KatWebX.
- March 31 - All KatWeb users will be told to upgrade to KatWebX.
- June 13 - KatWeb is given EOL status, and is discontinued. For users who still rely on KatWeb, per-person upgrade support and additional patches to KatWeb will be available on request until December 16, 2019.

## Current Features
- Easy to read TOML configuration
- Flexible configuration parsing
- Regex-based redirects
- Compressed regex-based reverse proxy
- Websocket reverse proxying
- HTTP basic authentication
- Fast file serving
- Brotli file compression
- Systemd/systemfd socket listening
- HSTS support
- SNI and OCSP reponse stapling
- High peformance HTTP/2 and TLS 1.3
- Multiple logging types
- Material design server-generated pages

## Possible Features
- On-the-fly config reloading (Currently extremely difficult to implement, requires rewrite of configuration handling)
- QUIC support (Will be implemented after [actix-web issue #309](https://github.com/actix/actix-web/issues/309) is closed)
- TLS mutual auth (Likely to be implemented in the near future)
- FastCGI support (Unlikely to be implemented in the near future, lack of existing client libraries)
- Let's Encrypt integration (Difficult but practical to implement, possible in the future)
- Caching proxy (Currently very difficult to implement, unlikely to be implemented in the near future)
- Advanced load balancer (Likely to be implemented in the near future)
