use criterion::{black_box, criterion_group, criterion_main, Criterion};
use hl_types::{normalize_coin, parse_mid_price_from_l2book, parse_str_decimal};

fn bench_normalize_coin(c: &mut Criterion) {
    c.bench_function("normalize_coin_uppercase_no_suffix", |b| {
        b.iter(|| normalize_coin(black_box("BTC")))
    });
    c.bench_function("normalize_coin_with_suffix", |b| {
        b.iter(|| normalize_coin(black_box("BTC-PERP")))
    });
    c.bench_function("normalize_coin_lowercase_suffix", |b| {
        b.iter(|| normalize_coin(black_box("btc-perp")))
    });
}

fn bench_parse_str_decimal(c: &mut Criterion) {
    let val = serde_json::json!("90000.50");
    c.bench_function("parse_str_decimal_string", |b| {
        b.iter(|| parse_str_decimal(black_box(Some(&val)), "px"))
    });
}

fn bench_parse_mid_price(c: &mut Criterion) {
    let json = serde_json::json!({
        "levels": [
            [{"px": "90000.0", "sz": "1.0"}, {"px": "89999.0", "sz": "2.0"}],
            [{"px": "90001.0", "sz": "0.5"}, {"px": "90002.0", "sz": "1.5"}]
        ]
    });
    c.bench_function("parse_mid_price_from_l2book", |b| {
        b.iter(|| parse_mid_price_from_l2book(black_box(&json)))
    });
}

criterion_group!(
    benches,
    bench_normalize_coin,
    bench_parse_str_decimal,
    bench_parse_mid_price
);
criterion_main!(benches);
