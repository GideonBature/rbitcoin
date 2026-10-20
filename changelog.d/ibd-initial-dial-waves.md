Fixed

- **Catch-up dials the next wave when the first reaches no one.** One
  initial batch that failed ended catch-up with `no peers connected` while
  the book still held untried addresses. IBD now dials fresh waves until a
  peer connects or the book is exhausted.
