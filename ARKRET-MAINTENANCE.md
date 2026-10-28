# Matrix Bot SDK security maintenance

This repository imports only the publicly released crates.io package
`matrix-bot-sdk` 0.2.4, not the private upstream repository's history.
Published source: https://crates.io/crates/matrix-bot-sdk/0.2.4
Upstream: https://github.com/palpo-im/matrix-bot-sdk
Published upstream revision: 2d34e639b63f75cd2fc441ce133d6ec4d0c385d5
The original Apache-2.0 license remains in LICENSE.

The `arkret/salvo1-security` branch upgrades the appservice HTTP framework to
Salvo 1.0.1. That selects the maintained zstd 0.14 dependency family instead
of the vulnerable zstd 0.13 retained by Salvo 0.89. No appservice features,
request handling, authentication or compression functionality are removed.
Consumers must select this branch's reviewed full commit SHA.
