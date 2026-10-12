Fixed

- **A below-floor header batch is ignored without reading parent headers.** Peers that are not noban no longer walk the header store before that ignore. Noban peers still persist and sync.
- **A published tip is in the height index without rereading its header.** Catch-up reads the missing headers before it takes the index lock. Disconnect still drops heights above the new tip.
- **Header batches are persisted off the IBD coordination loop.** That loop copies finished header rows and still disconnects a stalled peer on the same pass.
- **A requested body and a confirm reject carry the header facts the coordination loop needs.** That loop does not look up a header fk or a reject parent. A body with no fk is not stored.
- **The IBD progress tick reads published counters.** Tip, tx count, scripthash runs, and process-owned sizes come from values other threads already published. The tick does not list the scripthash directory or lock the height index.
- **A tip hole in front of later queued bodies is requested.** The densify cursor rewinds to the first height that is not claim-ready. A holder whose peer is gone does not count as covering that height. A full getdata window still rewinds that cursor. Heights already found claim-ready are not walked again until the tip moves back or a holder is freed.
- **The coordination loop does not replay header rows it already consumed.** A confirmed, rejected, or rewound hash stays off the work queue. The header-fk map is extended in place, not copied, and that publish happens after the coordination lock drops.
- **Header-store work on the header thread runs outside the coordination lock.** A short header batch, a resume, an origin load, a reseed's block check, and a most-work rewind do their store IO before the lock is taken again for the RAM commit.
- **A confirm reject whose parent header is missing is a cascade, not a consensus ban.**
- **IBD progress counters are seeded before the first tick.** The ordered-queue size on that tick is the live deque. A below-floor header batch does not move the peer's best-known tip, does not ask for the next header batch, and uses cached tip work when the prefix is already warm.
- **A multi-block confirm indexes every new height from the rows it already has.** The header command channel is bounded so a slow header thread cannot queue an unlimited number of batches on the coordination loop.
