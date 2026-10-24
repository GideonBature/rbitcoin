Fixed

- **The Esplora request limit bounds the listener, not each route.**
  axum's `Router::layer` wraps every route on its own, so
  `ConcurrencyLimitLayer` gave each of the listener's routes its own 256
  permits: in practice no limit. The limit is now one semaphore shared by
  every route and the fallback. Requests over it wait for a slot; they are
  not refused.

Added

- **`--esplora-max-inflight N`** (conf `esplora_max_inflight=`, NixOS
  `services.rbitcoin.esplora.maxInflight`) sets that limit. The default stays
  256. A busy public listener, or one serving silent-payment rescans, may
  want more.
