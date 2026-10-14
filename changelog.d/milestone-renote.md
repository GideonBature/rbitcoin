Fixed

- **IBD restart keeps script checks off on the stored header path.** After the anchor is restored, headers the resume seed queues from the store are noted on the milestone path again, so blocks under that anchor are not script-checked a second time.
