Fixed

- **Electrum balance and Esplora mempool totals keep a just-confirmed spend until the scripthash view includes its block.** Those reads used to drop the spend as soon as the chain knew the transaction id, one moment before confirmed history could see it.
- **Electrum balance subtracts a spend of a confirmed parent that is still in the mempool.** With relay off, that parent stays pooled after its block is in the scripthash view. The unconfirmed total used to add the new outputs on top of the confirmed parent.
