Fixed

- **Header-cache cleanup keeps the height and parent of stored path headers still waiting for the IBD download queue.** The queue can refill from them.
- **A new lookahead header batch does not jump ahead of stored headers waiting for queue space.** Those slots stay for the next contiguous headers.
- **Only an empty reconnect round with no live IBD peer spends the reconnect budget.** A replacement connection that already started is allowed to finish. A spent budget still stops IBD without entering tip mode.
- **A follow-session peer that goes over the message or byte rate budget is disconnected on that frame.** Its unanswered block asks are released when the session ends, so another peer can be asked right away. A second rate-limit disconnect from the same address within an hour refuses that address for a day.
