Fixed an IBD stall when a new lookahead header batch jumped ahead of stored headers waiting for download queue space. Header intake now leaves those slots for the next contiguous successors.
