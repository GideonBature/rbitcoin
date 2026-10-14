Changed

- **During IBD one peer is asked for the next header that is not stored.** While the queue has room the locator starts at the stored tip, then the candidate and a lighter fork. A full queue asks from the candidate. A refused low-work batch is not paged. Only one connecting peer starts headers sync. An unknown block inv from outside the header slot does not retire the reserved peer. After IBD, connect-time headers sync is unchanged.
