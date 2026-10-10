Fixed

- **A peer dropped for missed header asks, off-path headers, or a bad block hands back its block requests.** These disconnects marked the peer dead without releasing its block getdata, and the main loop dropped the slot before the peer's dead event arrived, so nothing ever released them. Those heights stayed requested from no peer. A tip hole behind them waited for a racer, and with no racer IBD stopped.
