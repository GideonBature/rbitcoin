Fixed

- IBD retires peers that repeat full header batches without advancing the candidate, download path, or a competing chain. An unsubstantiated advertised height no longer keeps a caught-up node waiting on those replies; full batches that advance a competing chain remain eligible.
