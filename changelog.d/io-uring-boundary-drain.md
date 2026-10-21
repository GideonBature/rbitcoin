Fixed

- **Completion-session boundaries never drain SQEs whose buffers may be gone.**
  `begin_batch`, `with_thread_local` entry/exit/growth, and session `Drop` used
  to drain leftover `pending` ids from a previous frame. That frame's buffers
  were already freed, so a successful drain could write completions into freed
  memory. Those boundaries now abort instead. Only the frame that owns the
  buffers drains, or a `DrainOnDrop` declared after them; caller-frame and
  backstop guards use the new abort-only `abort_guard`.
