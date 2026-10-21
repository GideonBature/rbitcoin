Fixed

- **Completion-session drains fail closed when SQEs are still pending.**
  `drain_all` could return an error with ids still in `pending`, and
  `drop_drain` ignored that error, so a caller could free SQE buffers while
  the kernel or pool worker still owned them. A failed drain with pending ids
  now aborts (the debug `with_drain_hard_cap` seam still returns the error),
  matching the hard-cap behavior. The `DrainOnDrop` guidance is corrected:
  guards sharing a buffer frame are declared after those buffers.
