Fixed

- **Completion-session buffers are never freed with SQEs still in flight.**
  A panic inside `with_thread_local` unwound past the drain and left the
  thread-local session pointing at freed buffers; the next call drained the
  kernel into them. Drop guards and session drops now abort when they see
  in-flight SQEs during an unwind, and `with_thread_local` aborts as a
  backstop instead of reusing the session. `index_build_uring::run_on_session`
  likewise drains leftover SQEs before returning an error, while its
  `ReadJob` buffers are still alive, and keeps the original build error.
