Fixed

- **IBD connects archived blocks that spend a create from the same batch.**
  A retry from archived bodies found no range for a parent created in that
  batch, logged `lookup stage miss`, and halted IBD below valid blocks. The
  retry follows an invalid block in a batch, or a restart after a crash
  between the Class A write and the tip advance.
