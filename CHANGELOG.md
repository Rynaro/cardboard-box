# Changelog

## [0.11.0](https://github.com/Rynaro/cardboard-box/compare/v0.10.0...v0.11.0) (2026-06-20)


### Features

* **tui:** bundle 3 "glass cockpit" — live log streaming, scroll mouse, action history ([#34](https://github.com/Rynaro/cardboard-box/issues/34)) ([44a366f](https://github.com/Rynaro/cardboard-box/commit/44a366f373d014418b7053abbdccf9769db9fff8))

## [0.10.0](https://github.com/Rynaro/cardboard-box/compare/v0.9.0...v0.10.0) (2026-06-20)


### Features

* **tui:** bundle 2 "living box" — sparklines, auto-refresh, palette, bulk ops ([#32](https://github.com/Rynaro/cardboard-box/issues/32)) ([da27c44](https://github.com/Rynaro/cardboard-box/commit/da27c44893f209a9058665bc11833f7a5c03fefc))

## [0.9.0](https://github.com/Rynaro/cardboard-box/compare/v0.8.0...v0.9.0) (2026-06-20)


### Features

* **tui:** bundle 1 "retro cockpit" — filter, cheatsheet, skins, toasts, command-log ([#30](https://github.com/Rynaro/cardboard-box/issues/30)) ([ad4f24d](https://github.com/Rynaro/cardboard-box/commit/ad4f24d8845bf04f1fc7aad95cd7666732136a31))

## [0.8.0](https://github.com/Rynaro/cardboard-box/compare/v0.7.0...v0.8.0) (2026-06-19)


### Features

* **tui:** themed retro UI with brand header and no-color fallback ([#28](https://github.com/Rynaro/cardboard-box/issues/28)) ([5d43eb4](https://github.com/Rynaro/cardboard-box/commit/5d43eb4148c725295360209be69c3880745b975f))

## [0.7.0](https://github.com/Rynaro/cardboard-box/compare/v0.6.0...v0.7.0) (2026-06-19)


### Features

* **secrets:** native keyring-backed secret management ([#24](https://github.com/Rynaro/cardboard-box/issues/24)) ([d43d918](https://github.com/Rynaro/cardboard-box/commit/d43d91886032484df76b6e9bdf7912c81ee99f6a))

## [0.6.0](https://github.com/Rynaro/cardboard-box/compare/v0.5.2...v0.6.0) (2026-06-19)


### Features

* **cli:** add stop command and fix rm on running boxes ([#22](https://github.com/Rynaro/cardboard-box/issues/22)) ([86d82b5](https://github.com/Rynaro/cardboard-box/commit/86d82b59b036d77c56eb46562312057556edfd45))

## [0.5.2](https://github.com/Rynaro/cardboard-box/compare/v0.5.1...v0.5.2) (2026-06-19)


### Bug Fixes

* **diff:** normalize mount mode so default rw is not a spurious recreate ([#20](https://github.com/Rynaro/cardboard-box/issues/20)) ([e90f19d](https://github.com/Rynaro/cardboard-box/commit/e90f19d2c4fe4b78c3d8d7180d3812c676113947))

## [0.5.1](https://github.com/Rynaro/cardboard-box/compare/v0.5.0...v0.5.1) (2026-06-18)


### Bug Fixes

* correct boxfile↔live diff (image/home/mounts) and backend resolution ([#17](https://github.com/Rynaro/cardboard-box/issues/17)) ([b93776d](https://github.com/Rynaro/cardboard-box/commit/b93776da338aaf82e5854a3ecc44d4c7aab72b69))

## [0.5.0](https://github.com/Rynaro/cardboard-box/compare/v0.4.1...v0.5.0) (2026-06-17)


### Features

* unified cross-backend box listing and per-box routing ([0cedfee](https://github.com/Rynaro/cardboard-box/commit/0cedfee093a2d336aa0841fb2b9ae828b407fd2d))
* unified cross-backend box listing and per-box routing ([8335a5e](https://github.com/Rynaro/cardboard-box/commit/8335a5eed3648fc5e3d5b32f423a20298ec92fe6))

## [0.4.1](https://github.com/Rynaro/cardboard-box/compare/v0.4.0...v0.4.1) (2026-06-17)


### Bug Fixes

* parse docker NDJSON ps output when listing boxes ([1f1eed5](https://github.com/Rynaro/cardboard-box/commit/1f1eed5fce0a03c921f7bf6f532d752b50557125))
* probe for usable backend in the TUI instead of defaulting to podman ([f67a472](https://github.com/Rynaro/cardboard-box/commit/f67a47296f739a4532487a0ad74cc9833d94be52))
* TUI lists docker-backed distroboxes again ([1b92474](https://github.com/Rynaro/cardboard-box/commit/1b9247487f18ebcd6d2800656e2aa15791bef38f))

## [0.4.0](https://github.com/Rynaro/cardboard-box/compare/v0.3.1...v0.4.0) (2026-06-17)


### Features

* surface provision step failures Vagrant-style ([#9](https://github.com/Rynaro/cardboard-box/issues/9)) ([a859ae4](https://github.com/Rynaro/cardboard-box/commit/a859ae416712fe7586bf3631f96a8d421e0d974d))

## [0.3.1](https://github.com/Rynaro/cardboard-box/compare/v0.3.0...v0.3.1) (2026-06-17)


### Bug Fixes

* expand XDG state path when writing provision state ([#7](https://github.com/Rynaro/cardboard-box/issues/7)) ([501e5fb](https://github.com/Rynaro/cardboard-box/commit/501e5fbc91ac2631b87ce4a362e4068549fb8ae2))

## [0.3.0](https://github.com/Rynaro/cardboard-box/compare/v0.2.1...v0.3.0) (2026-06-17)


### Features

* auto-discover Boxfile.toml in the current directory ([#5](https://github.com/Rynaro/cardboard-box/issues/5)) ([2a2d4c5](https://github.com/Rynaro/cardboard-box/commit/2a2d4c5a8b33baeee611cbe233929e9c3e46e11f))

## [0.2.1](https://github.com/Rynaro/cardboard-box/compare/v0.2.0...v0.2.1) (2026-06-17)


### Bug Fixes

* select docker-mode packages by image distro family ([#3](https://github.com/Rynaro/cardboard-box/issues/3)) ([ec47895](https://github.com/Rynaro/cardboard-box/commit/ec47895383153225e707ca0ad9e07d7abb174811))

## [0.2.0](https://github.com/Rynaro/cardboard-box/compare/v0.1.0...v0.2.0) (2026-06-17)


### Miscellaneous Chores

* release 0.2.0 ([63662bf](https://github.com/Rynaro/cardboard-box/commit/63662bf0b9e626d7b87d13f4a5f08325bf8f47c4))

## [0.1.0](https://github.com/Rynaro/cardboard-box/compare/v0.1.0...v0.1.0) (2026-06-17)


### Miscellaneous Chores

* release 0.1.0 ([6fdc006](https://github.com/Rynaro/cardboard-box/commit/6fdc0063af5a7257398a7d84a3007e96a2953aaa))
