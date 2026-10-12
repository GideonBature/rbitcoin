Fixed

- **A below-floor header batch is ignored without reading parent headers.** Peers that are not noban no longer walk the header store before that ignore. Noban peers still persist and sync.
- **A published tip is in the height index without rereading its header.** Catch-up reads the missing headers before it takes the index lock. Disconnect still drops heights above the new tip.
- **Header batches are persisted off the IBD coordination loop.** That loop copies finished header rows and still disconnects a stalled peer on the same pass.
