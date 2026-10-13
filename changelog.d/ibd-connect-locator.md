Fixed

- **During IBD, a connecting peer is asked for headers from the header-walk tip.** The confirmed chain stays later in the locator. After IBD, the locator starts at the confirmed tip again.
