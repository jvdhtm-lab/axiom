---
title: HTTP server core
description: The axiom-server HTTP boundary, route registration, ApiGuard enforcement, and its current limitations.
---

# HTTP server core

`axiom-server` is the first slice of Phase 14. It is a small HTTP/1.1 boundary
that enforces [`ApiGuard`](security-model.md) before a handler runs and bridges
JSON to the runtime `Value` model.

It does **not** compile Axiom handlers, generate typed browser stubs, terminate
TLS, or run the reference `get_dashboard` route yet. Those remain in Phase 14/15
of the [remaining phases](remaining-phases.md).

## What the server does

- Registers explicit `method` + `path` routes and maps each route to an exported
  API name.
- Resolves the caller with an injected authenticator. A principal is never read
  from the request body.
- Calls `ApiGuard::authorize` before the handler. Missing policy, missing
  authentication, missing capability, and tenant mismatch fail closed.
- Decodes JSON request bodies into `Value` and encodes `Value` responses.
- Bounds requests to 1 MiB and handles one request per connection.
- Returns stable JSON errors: `unauthenticated` (401), `forbidden`/`missing_policy`
  (403), `not_found` (404), `method_not_allowed` (405), `invalid_json` (400).

## Library example

```rust
use std::sync::Arc;
use axiom_runtime::{ApiGuard, ApiPolicy, Principal, Value};
use axiom_server::{Request, Server, ServerError};

let mut guard = ApiGuard::new();
guard.register(ApiPolicy {
    name: "get_dashboard".to_string(),
    authentication_required: true,
    required_capabilities: vec!["dashboard:read".to_string()],
    data_class: Some("internal".to_string()),
    version: Some("1.0.0".to_string()),
});

let server = Server::new(
    guard,
    Arc::new(|request: &Request| {
        // Replace with real identity-provider verification.
        request.header("authorization").map(|token| {
            Principal::new(token, "acme").with_capability("dashboard:read")
        })
    }),
)
.route(
    "POST",
    "/api/get_dashboard",
    "get_dashboard",
    |_request, principal, body| {
        let _ = principal;
        Ok(Value::Record(vec![
            ("ok".to_string(), Value::Bool(true)),
            ("echo".to_string(), body),
        ]))
    },
);

let listener = std::net::TcpListener::bind("127.0.0.1:8080")?;
server.serve(listener)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Request:

```sh
curl -i http://127.0.0.1:8080/api/get_dashboard \
  -H 'authorization: alice' \
  -H 'content-type: application/json' \
  -d '{"team_id":"team-1"}'
```

Without a principal the response is `401`; with a principal that lacks
`dashboard:read` it is `403`; the handler only runs after the guard passes.

## JSON value mapping

| Value | JSON |
| --- | --- |
| `Unit` | `null` |
| `Bool`, `Int`, `Float`, `Str` | primitive |
| `Bytes` | `{"$bytes":"hex"}` |
| `Duration` | `{"$duration_ms":n}` |
| `Timestamp` | `{"$timestamp":n}` |
| `Variant` | `{"$variant":"Name","$args":[...]}` |
| `Record` | object |
| `List` | array |
| `Function` | not serializable |

## Tests

```sh
cargo test -p axiom-server
```

The suite covers JSON round-trips, authentication/capability refusals, missing
policy, unknown routes, method mismatch, malformed JSON, and a real TCP request.

## Limitations

- No TLS. Run behind a TLS-terminating proxy.
- No CORS, keep-alive, chunked transfer encoding, compression, or streaming.
- No identity-provider integration; the authenticator is injected.
- No route wiring to evaluated Axiom handlers yet.
- No typed browser API stubs yet.
- No rate limiting or metrics.
