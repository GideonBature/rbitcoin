Fixed

- **IBD block accept and confirm lookup no longer share one body-queue lock.** Height rows are striped, hash presence is its own index, and the decoded stash is a third lock. A lookup wave cannot stall getdata into RAM, and a presence check cannot stall lookup.
