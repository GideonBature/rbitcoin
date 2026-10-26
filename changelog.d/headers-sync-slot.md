Fixed

- **Headers-sync stall check waits for the inbound slot.** The journey
  dialed a preferred outbound before the inbound owned the only initial
  headers-sync slot. Under coverage that outbound can win the slot, a sole
  preferred peer is kept, and the coverage job failed while the test job
  passed.
