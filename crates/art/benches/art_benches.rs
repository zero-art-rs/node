use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::{Duration, Instant};

use ark_bn254::{G2Projective as ART_G, fr::Fr as ARTScalarField};
use ark_ec::{CurveGroup, PrimeGroup, pairing::Pairing};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::UniformRand;
use art::helper_tools::create_random_secrets;
use art::{art::ART, art_user_agent::ARTUserAgent};
use rand::{Rng, thread_rng};

// hardcoded number of leaves in a tree for testing
// pub const TEST_SAMPLES: [usize; 5] = [100, 200, 300, 400, 500];
pub const TEST_SAMPLES: [usize; 3] = [5, 10, 15];

pub fn get_two_user_agents<G: CurveGroup + CanonicalSerialize + CanonicalDeserialize>(
    group_size: usize,
) -> (ARTUserAgent<G>, ARTUserAgent<G>) {
    let secrets = create_random_secrets(group_size);
    let (mut tree, root_key) = ART::new_art_from_secrets(&secrets, &G::generator());

    let user_agent1 = ARTUserAgent::new(tree.clone(), secrets[0]);
    let user_agent2 = ARTUserAgent::new(tree.clone(), secrets[1]);

    (user_agent1, user_agent2)
}

pub fn get_several_user_agents<G: CurveGroup + CanonicalSerialize + CanonicalDeserialize>(
    group_size: usize,
    number_of_agents: usize,
) -> Vec<ARTUserAgent<G>> {
    let secrets = create_random_secrets(group_size);

    let mut tree = ART::new_art_from_secrets(&secrets, &G::generator()).0;

    let mut agents = Vec::new();
    for i in 0..number_of_agents {
        agents.push(ARTUserAgent::new(tree.clone(), secrets[i]));
    }

    agents
}

pub fn iter_with_revert<G, F1, F2>(
    iters: u64,
    user: &mut ARTUserAgent<G>,
    f_run: F1,
    f_rev: F2,
) -> Duration
where
    G: CurveGroup + CanonicalSerialize + CanonicalDeserialize,
    F1: Fn(&mut ARTUserAgent<G>) -> (),
    F2: Fn(&mut ARTUserAgent<G>) -> (),
{
    let mut revert_time = Duration::new(0, 0);
    let start = Instant::now();
    for _i in 0..iters {
        black_box(f_run(user));

        let start_revert = Instant::now();
        f_rev(user);
        revert_time += start_revert.elapsed();
    }
    start.elapsed() - revert_time
}

pub fn compute_art_and_ciphertexts_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("ART creation From secrets");
    for group_size in TEST_SAMPLES.iter() {
        group.throughput(Throughput::Elements(*group_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(group_size),
            group_size,
            |b, &group_size| {
                let secrets = create_random_secrets(group_size);

                b.iter(|| ART::new_art_from_secrets(&secrets, &ART_G::generator()))
            },
        );
    }

    group.finish();
}

pub fn art_serialise_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("ART");

    for group_size in TEST_SAMPLES.iter() {
        group.throughput(Throughput::Elements(*group_size as u64));
        group.bench_with_input(
            BenchmarkId::new("serialise", group_size),
            group_size,
            |b, &group_size| {
                let secrets = create_random_secrets(group_size);
                let mut tree = ART::new_art_from_secrets(&secrets, &ART_G::generator()).0;

                b.iter(|| tree.serialise())
            },
        );
    }

    for group_size in TEST_SAMPLES.iter() {
        group.throughput(Throughput::Elements(*group_size as u64));
        group.bench_with_input(
            BenchmarkId::new("deserialize", group_size),
            group_size,
            |b, &group_size| {
                let secrets = create_random_secrets(group_size);
                let mut tree = ART::new_art_from_secrets(&secrets, &ART_G::generator()).0;
                let art_json = tree.serialise().unwrap();

                b.iter(|| ART::<ART_G>::from_json(&art_json))
            },
        );
    }

    group.finish();
}

pub fn art_user_agent_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("ARTUserAgent");

    for group_size in TEST_SAMPLES.iter() {
        group.throughput(Throughput::Elements(*group_size as u64));
        group.bench_with_input(
            BenchmarkId::new("new", group_size),
            group_size,
            |b, &group_size| {
                let secrets = create_random_secrets(group_size);
                let mut tree = ART::new_art_from_secrets(&secrets, &ART_G::generator()).0;

                b.iter(|| {
                    ARTUserAgent::new(tree.clone(), secrets[thread_rng().gen_range(0..group_size)])
                });
            },
        );
    }

    for group_size in TEST_SAMPLES.iter() {
        group.throughput(Throughput::Elements(*group_size as u64));
        group.bench_with_input(
            BenchmarkId::new("update_key", group_size),
            group_size,
            |b, &group_size| {
                let secrets = create_random_secrets(group_size);
                let tree = ART::new_art_from_secrets(&secrets, &ART_G::generator()).0;

                let mut user_agent =
                    ARTUserAgent::new(tree, secrets[thread_rng().gen_range(0..group_size)]);

                b.iter(|| user_agent.update_key().unwrap())
            },
        );
    }

    for group_size in TEST_SAMPLES.iter() {
        group.throughput(Throughput::Elements(*group_size as u64));
        group.bench_with_input(
            BenchmarkId::new("update_branch after update_key", group_size),
            group_size,
            |b, &group_size| {
                let (mut user_agent1, mut user_agent2) = get_two_user_agents::<ART_G>(group_size);

                let (_, changes) = user_agent1.update_key().unwrap();

                b.iter(|| user_agent2.update_branch(&changes))
            },
        );
    }

    for group_size in TEST_SAMPLES.iter() {
        group.throughput(Throughput::Elements(*group_size as u64));
        group.bench_with_input(
            BenchmarkId::new("remove_node", group_size),
            group_size,
            |b, &group_size| {
                let secrets = create_random_secrets(group_size);
                let tree = ART::new_art_from_secrets(&secrets, &ART_G::generator()).0;

                let mut user_agent1 = ARTUserAgent::new(tree.clone(), secrets[0]);
                let mut user_agent2 = ARTUserAgent::new(tree, secrets[1]);

                b.iter_custom(move |iters| {
                    iter_with_revert(
                        iters,
                        &mut user_agent1,
                        |mut agent| {
                            _ = ARTUserAgent::remove_node(&mut agent, user_agent2.public_key())
                        },
                        |mut agent| _ = ARTUserAgent::append_node(&mut agent, user_agent2.lambda),
                    )
                })
            },
        );
    }

    for group_size in TEST_SAMPLES.iter() {
        group.throughput(Throughput::Elements(*group_size as u64));
        group.bench_with_input(
            BenchmarkId::new("update_branch after remove_node", group_size),
            group_size,
            |b, &group_size| {
                let secrets = create_random_secrets(group_size);
                let tree = ART::new_art_from_secrets(&secrets, &ART_G::generator()).0;

                let mut user_agent1 = ARTUserAgent::new(tree.clone(), secrets[0]);
                let mut user_agent2 = ARTUserAgent::new(tree.clone(), secrets[1]);
                let mut user_agent3 = ARTUserAgent::new(tree.clone(), secrets[2]);

                let (_, remove_changes) =
                    user_agent1.remove_node(user_agent2.public_key()).unwrap();
                let (_, append_changes) = user_agent1.append_node(user_agent2.lambda).unwrap();

                // b.iter(|| user_agent2.update_branch(&changes))
                b.iter_custom(move |iters| {
                    iter_with_revert(
                        iters,
                        &mut user_agent3,
                        |mut agent| _ = ARTUserAgent::update_branch(&mut agent, &remove_changes),
                        |mut agent| _ = ARTUserAgent::update_branch(&mut agent, &append_changes),
                    )
                })
            },
        );
    }

    for group_size in TEST_SAMPLES.iter() {
        group.throughput(Throughput::Elements(*group_size as u64));
        group.bench_with_input(
            BenchmarkId::new("make_temporal", group_size),
            group_size,
            |b, &group_size| {
                let (mut user_agent1, mut user_agent2) = get_two_user_agents::<ART_G>(group_size);

                b.iter_custom(move |iters| {
                    iter_with_revert(
                        iters,
                        &mut user_agent1,
                        |mut agent| {
                            _ = ARTUserAgent::make_temporal(&mut agent, user_agent2.public_key())
                        },
                        |mut agent| _ = ARTUserAgent::append_node(&mut agent, user_agent2.lambda),
                    )
                })
            },
        );
    }

    for group_size in TEST_SAMPLES.iter() {
        group.throughput(Throughput::Elements(*group_size as u64));
        group.bench_with_input(
            BenchmarkId::new("update_branch after make_temporal", group_size),
            group_size,
            |b, &group_size| {
                let mut users = get_several_user_agents::<ART_G>(group_size, 3);
                let mut user1 = users.pop().unwrap();
                let mut user2 = users.pop().unwrap();
                let mut user3 = users.pop().unwrap();

                let (_, make_temporal_changes) = user1.make_temporal(user2.public_key()).unwrap();
                let (_, append_changes) = user1.append_node(user2.lambda).unwrap();

                // b.iter(|| user_agent2.update_branch(&changes))
                b.iter_custom(move |iters| {
                    iter_with_revert(
                        iters,
                        &mut user3,
                        |mut agent| {
                            _ = ARTUserAgent::update_branch(&mut agent, &make_temporal_changes)
                        },
                        |mut agent| _ = ARTUserAgent::update_branch(&mut agent, &append_changes),
                    )
                })
            },
        );
    }

    for group_size in TEST_SAMPLES.iter() {
        group.throughput(Throughput::Elements(*group_size as u64));
        group.bench_with_input(
            BenchmarkId::new("append_node", group_size),
            group_size,
            |b, &group_size| {
                let mut agent = get_several_user_agents::<ART_G>(group_size, 1).remove(0);

                let lambda = ARTScalarField::rand(&mut thread_rng());

                b.iter(|| agent.append_node(lambda));
            },
        );
    }

    for group_size in TEST_SAMPLES.iter() {
        group.throughput(Throughput::Elements(*group_size as u64));
        group.bench_with_input(
            BenchmarkId::new("update_branch after append_node", group_size),
            group_size,
            |b, &group_size| {
                let mut agents = get_several_user_agents::<ART_G>(group_size, 2);

                let lambda = ARTScalarField::rand(&mut thread_rng());
                let public_key = agents[0].tree.public_key_of(lambda);

                let (_, append_changes) = agents[0].append_node(lambda).unwrap();
                let (_, make_temporal_changes) = agents[0].make_temporal(public_key).unwrap();

                let mut user = agents.remove(1);

                b.iter_custom(move |iters| {
                    iter_with_revert(
                        iters,
                        &mut user,
                        |mut agent| _ = ARTUserAgent::update_branch(&mut agent, &append_changes),
                        |mut agent| {
                            _ = ARTUserAgent::update_branch(&mut agent, &make_temporal_changes)
                        },
                    )
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    art_user_agent_benchmark,
    art_serialise_benchmark,
    compute_art_and_ciphertexts_benchmark,
);
criterion_main!(benches);
