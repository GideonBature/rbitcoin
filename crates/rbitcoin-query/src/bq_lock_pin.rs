//! Pins: getdata→BQ and lookup do not share a lock.
//!
//! Each test holds one side and requires the other side to finish. A single
//! body-queue mutex fails all of them.

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use super::Query;

fn open() -> Arc<Query> {
    static N: AtomicU64 = AtomicU64::new(0);
    let id = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("rbtc-bq-lock-{id}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Arc::new(Query::open_or_create_tiny(&dir).unwrap())
}

#[test]
fn getdata_enqueue_does_not_wait_on_lookup_resolved() {
    let q = open();
    let hold = q.bq_test_hold_resolved();
    let q2 = Arc::clone(&q);
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            q2.block_queue_enqueue(7, [7u8; 32], 1, b"payload").unwrap();
        }));
        let _ = tx.send(result);
    });
    let done = rx.recv_timeout(Duration::from_millis(400)).is_ok();
    drop(hold);
    if !done {
        let _ = rx.recv_timeout(Duration::from_secs(2));
    }
    worker.join().unwrap();
    assert!(done, "getdata enqueue waited on lookup's decoded stash");
}

#[test]
fn lookup_read_does_not_wait_on_getdata_hash_index() {
    let q = open();
    q.block_queue_enqueue(4, [4u8; 32], 1, b"raw").unwrap();
    let hold = q.bq_test_hold_hash_index();
    let q2 = Arc::clone(&q);
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            assert!(q2.block_queue_hash_at_height(4).is_some());
            assert!(q2.block_queue_raw_payload(4).unwrap().is_some());
            let intake = q2.block_queue_wave_intake(&[4]);
            assert_eq!(intake.raw.len(), 1);
        }));
        let _ = tx.send(result);
    });
    let done = rx.recv_timeout(Duration::from_millis(400)).is_ok();
    drop(hold);
    if !done {
        let _ = rx.recv_timeout(Duration::from_secs(2));
    }
    worker.join().unwrap();
    assert!(done, "lookup read waited on the getdata hash index");
}

#[test]
fn getdata_other_height_does_not_wait_on_lookup_stripe() {
    let q = open();
    q.block_queue_enqueue(1, [1u8; 32], 1, b"a").unwrap();
    let hold = q.bq_test_hold_height_stripe(1);
    let q2 = Arc::clone(&q);
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            q2.block_queue_enqueue(2, [2u8; 32], 1, b"b").unwrap();
            assert!(q2.block_queue_has_hash(&[2u8; 32]));
        }));
        let _ = tx.send(result);
    });
    let done = rx.recv_timeout(Duration::from_millis(400)).is_ok();
    drop(hold);
    if !done {
        let _ = rx.recv_timeout(Duration::from_secs(2));
    }
    worker.join().unwrap();
    assert!(
        done,
        "getdata on another height waited on lookup's height stripe"
    );
}

#[test]
fn has_hash_does_not_wait_on_height_stripe() {
    let q = open();
    q.block_queue_enqueue(3, [9u8; 32], 1, b"c").unwrap();
    let hold = q.bq_test_hold_height_stripe(3);
    let q2 = Arc::clone(&q);
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            assert!(q2.block_queue_has_hash(&[9u8; 32]));
        }));
        let _ = tx.send(result);
    });
    let done = rx.recv_timeout(Duration::from_millis(400)).is_ok();
    drop(hold);
    if !done {
        let _ = rx.recv_timeout(Duration::from_secs(2));
    }
    worker.join().unwrap();
    assert!(done, "has_hash waited on a height stripe");
}
