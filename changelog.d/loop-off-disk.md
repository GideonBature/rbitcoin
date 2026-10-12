Fixed

- **A below-floor header batch is ignored without reading parent headers.** Peers that are not noban no longer walk the header store before that ignore. Noban peers still persist and sync.
