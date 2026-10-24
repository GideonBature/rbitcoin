Fixed

- **The Esplora connection limit bounds the listener, not each route.**
  axum's `Router::layer` wraps every route on its own, so
  `ConcurrencyLimitLayer` gave each of the listener's routes its own
  `max_connections` permits: about 63 × 256 requests in flight by default.
  The limit is now one semaphore shared by every route, and the fallback.
