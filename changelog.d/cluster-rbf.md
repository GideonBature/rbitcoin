Fixed

- **A replacement may evict several mempool transactions that spend its
  unconfirmed inputs.** That is Core's cluster replacement. An unrelated
  unconfirmed input is still refused, and spending a conflict's own
  output is still `bad-txns-spends-conflicting-tx`. A replacement that
  would exceed the cluster cap is `too-large-cluster`.
