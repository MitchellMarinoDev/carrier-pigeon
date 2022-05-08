//! Benchmarks for downcasting.
#![feature(test)]

extern crate test;

use std::any::Any;
use test::Bencher;

#[bench]
fn downcast_unwrap(b: &mut Bencher) {
    let typed: (i32, &str, f64) = (1, "Hello", 3.14159);
    b.iter(|| {
        let any = Box::new(typed) as Box<dyn Any>;
        let downcast = any.downcast::<(i32, &str, f64)>().unwrap();
        assert_eq!(*downcast, typed);
    });
}
