Fixed

- **Script and IBD checks reject the boundary cases nightly mutants left open.** A 40-byte witness program is recognized, a null-parent genesis body is offered before the tip, a one-input coinbase is not stamped spent-by-non-coinbase, and CSV, stack, and opcode limits stay on the consensus side of the line.
