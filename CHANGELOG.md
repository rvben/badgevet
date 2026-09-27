# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [0.1.2](https://github.com/rvben/badgevet/compare/v0.1.1...v0.1.2) - 2026-09-27

### Fixed

- **deps**: update rustls to 0.23.45 for RUSTSEC-2026-0285 ([8256857](https://github.com/rvben/badgevet/commit/82568576ada0d77e56de1d958d8f5700f4e8609e))
- **ci**: install pinned Rust components ([d29bcfc](https://github.com/rvben/badgevet/commit/d29bcfc260ad152c574609a359413bb9a7e853a5))

## [0.1.0] - 2026-07-01

### Added

- add fix command to rewrite broken badges in place ([b4bdf08](https://github.com/rvben/badgevet/commit/b4bdf0881522f81b1706dd033f0c7221356daa2d))
- scan an owner's GitHub repos with --github ([e29bdbd](https://github.com/rvben/badgevet/commit/e29bdbdd0ab1297e55cdc6f635a20a3172c90966))
- implement badge health scanner ([1cc86e2](https://github.com/rvben/badgevet/commit/1cc86e28b112c2b2ca8589c9b68b939394b5e593))

### Fixed

- surface GitHub fetch failures and scope fix rewrites to badges ([38f94b2](https://github.com/rvben/badgevet/commit/38f94b27d15a3d8c8ba5b26928b1edf643a4a62c))
