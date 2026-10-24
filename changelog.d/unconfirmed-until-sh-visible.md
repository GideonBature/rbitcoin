Fixed

- **Electrum balance and Esplora mempool totals keep a just-confirmed spend until the scripthash view includes its block.** Those reads used to drop the spend as soon as the chain knew the transaction id, one moment before confirmed history could see it.
