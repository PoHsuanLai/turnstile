use turnstile_wire::limits::{EVENT_BURST, FLOOD_CUT_MS};
use turnstile_wire::{Admit, Limiter, Millis};

fn run(limiter: &mut Limiter, times: impl Iterator<Item = u64>) -> Vec<Admit> {
    times.map(|t| limiter.admit(Millis(t))).collect()
}

#[test]
fn a_pad_at_120_hertz_is_never_limited() {
    let mut l = Limiter::new(Millis(0));
    // One datagram every 9 ms is slower than the 8.33 ms the limit allows.
    let verdicts = run(&mut l, (0..2_000).map(|i| i * 9));
    assert!(verdicts.iter().all(|v| *v == Admit::Pass));
}

#[test]
fn a_burst_passes_then_the_rate_applies() {
    let mut l = Limiter::new(Millis(0));
    let verdicts = run(&mut l, std::iter::repeat_n(0, 100));
    let passed = verdicts.iter().filter(|v| **v == Admit::Pass).count();
    assert_eq!(passed as u64, EVENT_BURST);
    assert!(
        verdicts[EVENT_BURST as usize..]
            .iter()
            .all(|v| *v == Admit::Drop)
    );
}

#[test]
fn sustained_flooding_is_cut_after_three_seconds() {
    let mut l = Limiter::new(Millis(0));
    // 1000 datagrams per second, ten times the rate.
    let verdicts = run(&mut l, 0..=FLOOD_CUT_MS + 1_000);
    let first_cut = verdicts.iter().position(|v| *v == Admit::Cut);
    let at = first_cut.expect("a flood is cut") as u64;
    assert!(
        (FLOOD_CUT_MS..FLOOD_CUT_MS + 50).contains(&at),
        "cut at {at} ms"
    );
}

#[test]
fn a_pause_resets_the_flood_count() {
    let mut l = Limiter::new(Millis(0));
    // Two seconds of flooding, a two second pause, two more seconds: never three in a row.
    let burst = |from: u64| from..from + 2_000;
    let all: Vec<u64> = burst(0).chain(burst(4_000)).collect();
    let verdicts = run(&mut l, all.into_iter());
    assert!(verdicts.iter().all(|v| *v != Admit::Cut));
    assert!(verdicts.contains(&Admit::Drop));
}

#[test]
fn time_going_backwards_does_not_panic_or_refill() {
    let mut l = Limiter::new(Millis(10_000));
    let verdicts = run(&mut l, std::iter::repeat_n(5, 100));
    assert_eq!(
        verdicts.iter().filter(|v| **v == Admit::Pass).count() as u64,
        EVENT_BURST
    );
}
