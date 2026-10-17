#include <primitives/transaction.h>
#include <script/interpreter.h>
#include <streams.h>

#include <cstdint>
#include <span>

// Accept/reject only. 1 = script passes, 0 = script fails, 2 = bad input,
// 3 = Core threw (decode or an internal exception).
extern "C" int rbtc_core_verify(
    const unsigned char* tx_bytes,
    size_t tx_len,
    const unsigned char* spk,
    size_t spk_len,
    int64_t amount,
    unsigned int input_index,
    uint64_t flags)
{
    try {
        DataStream ds(std::span<const unsigned char>(tx_bytes, tx_len));
        CMutableTransaction mtx;
        ds >> TX_WITH_WITNESS(mtx);
        if (input_index >= mtx.vin.size()) {
            return 2;
        }
        CTransaction tx(mtx);
        CScript script_pub(spk, spk + spk_len);
        std::vector<CTxOut> spent;
        spent.emplace_back(amount, script_pub);
        PrecomputedTransactionData txdata;
        txdata.Init(tx, std::move(spent));
        TransactionSignatureChecker checker(
            &tx, input_index, amount, txdata, MissingDataBehavior::FAIL);
        ScriptError err = SCRIPT_ERR_UNKNOWN_ERROR;
        const CScriptWitness* wit = &tx.vin[input_index].scriptWitness;
        bool ok = VerifyScript(
            tx.vin[input_index].scriptSig,
            script_pub,
            wit,
            script_verify_flags::from_int(flags),
            checker,
            &err);
        return ok ? 1 : 0;
    } catch (...) {
        return 3;
    }
}
