Changed

- **IBD overlaps each batch's tip flush with the next batch.**
  The fsyncs that make a write batch's tip durable now run on a helper
  thread while the next batch is archived and checked. A crash can lose one
  more batch, which is downloaded again. A failed tip flush stops block
  confirmation until restart.
