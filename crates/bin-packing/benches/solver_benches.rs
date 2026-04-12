use bin_packing::one_d::{CutDemand1D, OneDAlgorithm, OneDOptions, OneDProblem, Stock1D, solve_1d};
use bin_packing::three_d::{
    Bin3D, BoxDemand3D, RotationMask3D, ThreeDAlgorithm, ThreeDOptions, ThreeDProblem, solve_3d,
};
use bin_packing::two_d::{
    RectDemand2D, Sheet2D, TwoDAlgorithm, TwoDOptions, TwoDProblem, solve_2d,
};
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};

fn benchmark_one_d(c: &mut Criterion) {
    let mut group = c.benchmark_group("solve_1d");

    for pieces in [20_usize, 50, 100] {
        let problem = OneDProblem {
            stock: vec![Stock1D {
                name: "bar".to_string(),
                length: 144,
                kerf: 1,
                trim: 2,
                cost: 1.0,
                available: None,
            }],
            demands: (0..pieces)
                .map(|index| CutDemand1D {
                    name: format!("piece_{index}"),
                    length: 12 + ((index * 7) % 36) as u32,
                    quantity: 1 + (index % 3),
                })
                .collect(),
        };

        let total_pieces = problem.demands.iter().map(|demand| demand.quantity).sum::<usize>();
        group.throughput(Throughput::Elements(total_pieces as u64));
        for algorithm in
            [OneDAlgorithm::BestFitDecreasing, OneDAlgorithm::LocalSearch, OneDAlgorithm::Auto]
        {
            group.bench_with_input(
                BenchmarkId::new(format!("{algorithm:?}"), pieces),
                &problem,
                |bench, problem| {
                    bench.iter(|| {
                        let _ = solve_1d(
                            black_box(problem.clone()),
                            OneDOptions { algorithm, seed: Some(13), ..OneDOptions::default() },
                        )
                        .expect("bench solve_1d should succeed");
                    });
                },
            );
        }
    }

    group.finish();
}

fn benchmark_two_d(c: &mut Criterion) {
    let mut group = c.benchmark_group("solve_2d");

    for count in [16_usize, 32, 64] {
        let problem = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 120,
                height: 96,
                cost: 1.0,
                quantity: None,
            }],
            demands: (0..count)
                .map(|index| RectDemand2D {
                    name: format!("panel_{index}"),
                    width: 10 + ((index * 5) % 28) as u32,
                    height: 8 + ((index * 11) % 20) as u32,
                    quantity: 1 + (index % 2),
                    can_rotate: index % 3 != 0,
                })
                .collect(),
        };

        let item_count = problem.demands.iter().map(|item| item.quantity).sum::<usize>();
        group.throughput(Throughput::Elements(item_count as u64));
        for algorithm in [
            TwoDAlgorithm::MaxRects,
            TwoDAlgorithm::Skyline,
            TwoDAlgorithm::Guillotine,
            TwoDAlgorithm::Auto,
        ] {
            group.bench_with_input(
                BenchmarkId::new(format!("{algorithm:?}"), count),
                &problem,
                |bench, problem| {
                    bench.iter(|| {
                        let _ = solve_2d(
                            black_box(problem.clone()),
                            TwoDOptions { algorithm, seed: Some(17), ..TwoDOptions::default() },
                        )
                        .expect("bench solve_2d should succeed");
                    });
                },
            );
        }
    }

    group.finish();
}

fn benchmark_three_d(c: &mut Criterion) {
    let mut group = c.benchmark_group("solve_3d");

    for count in [8_usize, 16, 32] {
        let problem = ThreeDProblem {
            bins: vec![Bin3D {
                name: "container".to_string(),
                width: 120,
                height: 80,
                depth: 60,
                cost: 1.0,
                quantity: None,
            }],
            demands: (0..count)
                .map(|index| BoxDemand3D {
                    name: format!("item_{index}"),
                    width: 8 + ((index * 7) % 20) as u32,
                    height: 6 + ((index * 11) % 16) as u32,
                    depth: 5 + ((index * 13) % 12) as u32,
                    quantity: 1 + (index % 2),
                    allowed_rotations: RotationMask3D::ALL,
                })
                .collect(),
        };

        let item_count = problem.demands.iter().map(|item| item.quantity).sum::<usize>();
        group.throughput(Throughput::Elements(item_count as u64));

        for algorithm in [
            ThreeDAlgorithm::ExtremePoints,
            ThreeDAlgorithm::Guillotine3D,
            ThreeDAlgorithm::LayerBuilding,
            ThreeDAlgorithm::MultiStart,
            ThreeDAlgorithm::Auto,
        ] {
            group.bench_with_input(
                BenchmarkId::new(format!("{algorithm:?}"), count),
                &problem,
                |bench, problem: &ThreeDProblem| {
                    bench.iter(|| {
                        let _ = solve_3d(
                            black_box(problem.clone()),
                            ThreeDOptions { algorithm, seed: Some(7), ..ThreeDOptions::default() },
                        )
                        .expect("bench solve_3d should succeed");
                    });
                },
            );
        }
    }

    group.finish();
}

criterion_group!(benches, benchmark_one_d, benchmark_two_d, benchmark_three_d);
criterion_main!(benches);
