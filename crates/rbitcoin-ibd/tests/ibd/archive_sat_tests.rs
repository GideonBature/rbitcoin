use rbitcoin_ibd::ibd::assign::bq_pipeline_saturated;

#[test]
fn bq_pipeline_saturated_gates_full_assign() {
    assert!(!bq_pipeline_saturated(0, false));
    assert!(!bq_pipeline_saturated(32, true));
    assert!(!bq_pipeline_saturated(0, false));
    assert!(bq_pipeline_saturated(0, true));
    assert!(bq_pipeline_saturated(15, true));
}
