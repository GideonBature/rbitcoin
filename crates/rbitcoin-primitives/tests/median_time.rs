use rbitcoin_primitives::median_time::*;

#[test]
fn unsorted_three() {
    let window = MedianWindow::new(&[3, 1, 2]).unwrap();
    assert_eq!(median_time_past_times(window), 2);
}

#[test]
fn even_count_takes_the_upper_middle() {
    let window = MedianWindow::new(&[1, 2, 3, 4]).unwrap();
    assert_eq!(median_time_past_times(window), 3);
}

#[test]
fn empty_median_time_is_an_error() {
    match MedianWindow::new(&[]) {
        Err(err) => assert!(err.contains("median"), "{err}"),
        Ok(_) => panic!("empty median window"),
    }
}
