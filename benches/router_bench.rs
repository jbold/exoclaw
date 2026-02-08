use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use exoclaw::router::{Binding, SessionRouter};

fn build_router(size: usize) -> SessionRouter {
    let mut router = SessionRouter::new();
    for i in 0..size {
        router.add_binding(Binding {
            agent_id: format!("agent-{i}"),
            channel: Some(format!("channel-{i}")),
            account_id: None,
            peer_id: None,
            guild_id: None,
            team_id: None,
        });
    }
    router
}

fn bench_router_resolve(c: &mut Criterion) {
    let mut group = c.benchmark_group("router_resolve");

    for size in [100usize, 1_000, 10_000] {
        let mut router = build_router(size);
        let channel = format!("channel-{}", size - 1);

        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| {
                let route = router.resolve(
                    black_box(&channel),
                    black_box("acct"),
                    Some("peer"),
                    None,
                    None,
                );
                black_box(route.session_key);
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_router_resolve);
criterion_main!(benches);
