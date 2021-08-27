[![CI](https://github.com/pkgcraft/pkgcraft/workflows/test/badge.svg)](https://github.com/pkgcraft/pkgcraft/actions/workflows/test.yml)
[![Coverage](https://codecov.io/gh/pkgcraft/pkgcraft/branch/main/graph/badge.svg)](https://codecov.io/gh/pkgcraft/pkgcraft)

# Pkgcraft

Pkgcraft is a highly experimental, rust-based, tooling ecosystem for Gentoo. It
aims to provide bindings for other programming languages targeting
Gentoo-specific functionality as well as a new approach to package management,
leveraging a client-server design that will potentially support various
frontends.

## Components

- **pkgcraft**: core library supporting various Gentoo-related functionality
- **arcanist**: daemon focused on package querying, building, and merging
- **pakt**: command-line client for arcanist

## Requirements

Minimum supported rust version: 1.54.0

## Contact

For bugs and feature requests please create an [issue][1].

Otherwise [discussions][2] can be used for general questions and support.

[1]: <https://github.com/pkgcraft/pkgcraft/issues>
[2]: <https://github.com/pkgcraft/pkgcraft/discussions>
