Fixed an IBD stall when header-cache cleanup removed metadata for stored headers still waiting beyond the download queue. Pending path headers now keep their height and parent mappings until queued.
