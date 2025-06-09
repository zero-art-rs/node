#[cfg(test)]
mod tests {
    use ark_bn254::{G2Projective as ART_G, fr::Fr as ARTScalarField};
    use ark_ec::PrimeGroup;
    use ark_ec::pairing::Pairing;
    use ark_std::UniformRand;
    use art::art_user_agent::ARTUserAgent;
    use art::{self, art::ART, helper_tools};
    use helper_tools::create_random_secrets;
    use rand::{Rng, thread_rng};

    #[test]
    fn test_art_tree_key_update() {
        let number_of_users = 100;
        let main_user_id = thread_rng().gen_range(0..number_of_users);
        let secrets = create_random_secrets(number_of_users);

        let (mut tree, root_key) = ART::new_art_from_secrets(&secrets, &ART_G::generator());

        let mut users_agents = Vec::new();
        for i in 0..number_of_users {
            users_agents.push(ARTUserAgent::new(tree.clone(), secrets[i]));
        }

        for user_agent in &users_agents {
            // Assert admin and users computed the same tree key.
            assert_eq!(user_agent.root_key.key, root_key.key);
        }

        let mut main_user_agent = users_agents.remove(main_user_id);

        // save old lambda to roll back
        let old_lambda = main_user_agent.lambda;
        let (new_key, changes) = main_user_agent.update_key().unwrap();

        for user_agent in &users_agents {
            assert_ne!(root_key.key, new_key.key);
        }

        for user_agent in &mut users_agents {
            _ = user_agent.update_branch(&changes);
            assert_eq!(user_agent.root_key.key, new_key.key);
        }

        let (old_key, changes) = main_user_agent.change_lambda(old_lambda).unwrap();

        assert_eq!(root_key.key, old_key.key);

        for user_agent in &mut users_agents {
            _ = user_agent.update_branch(&changes);
            assert_eq!(user_agent.root_key.key, old_key.key);
        }
    }

    #[test]
    fn test_art_tree_serialisation() {
        let number_of_users = 100;
        let main_user_id = thread_rng().gen_range(0..number_of_users as usize);
        let secrets = create_random_secrets(number_of_users);

        let (tree, root_key) = ART::new_art_from_secrets(&secrets, &ART_G::generator());

        let serialized = serde_json::to_string(&tree).unwrap();
        let deserialized: ART<ART_G> = serde_json::from_str(&serialized).unwrap();

        assert!(deserialized.eq(&tree));
    }

    #[test]
    fn test_art_make_temporal_node() {
        let number_of_users = 100;
        let main_user_id = thread_rng().gen_range(0..(number_of_users - 2) as usize);

        let secrets = create_random_secrets(number_of_users);
        let generator = ART_G::generator();

        let mut temporal_user_id = thread_rng().gen_range(0..(number_of_users - 3) as usize);
        while temporal_user_id >= main_user_id && temporal_user_id <= main_user_id + 2 {
            temporal_user_id = thread_rng().gen_range(0..(number_of_users - 3) as usize);
        }

        let (mut tree, root_key) = ART::new_art_from_secrets(&secrets, &ART_G::generator());

        let mut users_agents = Vec::new();
        for i in 0..number_of_users {
            users_agents.push(ARTUserAgent::new(tree.clone(), secrets[i]));
        }

        for user_agent in &users_agents {
            // Assert trusted party and users computed the same tree key. Skip lambda because trusted party cant compute it
            assert_eq!(user_agent.root_key.key, root_key.key);
        }

        let mut main_user_agent = users_agents.remove(main_user_id);
        let mut temporal_user_agent = users_agents.remove(temporal_user_id);

        let (root_key, changes) = main_user_agent
            .make_temporal(temporal_user_agent.public_key())
            .unwrap();

        for user_agent in &mut users_agents {
            assert_ne!(user_agent.root_key.key, main_user_agent.root_key.key);

            _ = user_agent.update_branch(&changes);

            assert_eq!(user_agent.root_key.key, main_user_agent.root_key.key);
            assert_eq!(user_agent.tree.size(), (number_of_users - 1) as usize);
        }

        let mut rng = thread_rng();
        let new_lambda = ARTScalarField::rand(&mut rng);

        let (root_key, changes) = main_user_agent.append_node(new_lambda).unwrap();

        for user_agent in &mut users_agents {
            _ = user_agent.update_branch(&changes);

            assert_eq!(user_agent.root_key.key, main_user_agent.root_key.key);
            assert_eq!(user_agent.tree.size(), number_of_users as usize);
        }
    }

    #[test]
    fn test_art_node_removal() {
        let number_of_users = 100;
        let secrets = create_random_secrets(number_of_users);

        let temporal_user_id = thread_rng().gen_range(3..number_of_users as usize);

        let (mut tree, root_key) = ART::new_art_from_secrets(&secrets, &ART_G::generator());

        let mut users_agents = Vec::new();
        for i in 0..number_of_users {
            users_agents.push(ARTUserAgent::new(tree.clone(), secrets[i]));
        }

        for user_agent in &users_agents {
            // Assert admin and users computed the same tree key.
            assert_eq!(user_agent.root_key.key, root_key.key);
        }

        let mut main_user_agent = users_agents.remove(0);
        let mut main_user_neighbour = users_agents.remove(0);
        for i in 0..2 {
            let mut for_removal = users_agents.remove(0);

            let (root_key, changes) = main_user_agent
                .remove_node(for_removal.public_key())
                .unwrap();

            for user_agent in &mut users_agents {
                assert_ne!(user_agent.root_key.key, main_user_agent.root_key.key);

                _ = user_agent.update_branch(&changes);

                assert_eq!(user_agent.root_key.key, main_user_agent.root_key.key);
                assert_eq!(user_agent.tree.size(), (number_of_users - 1 - i) as usize);
            }
        }

        assert!(!main_user_agent.can_remove(users_agents[0].public_key()));

        let (root_key, changes) = main_user_agent
            .remove_node(main_user_neighbour.public_key())
            .unwrap();

        for user_agent in &mut users_agents {
            assert_ne!(user_agent.root_key.key, main_user_agent.root_key.key);

            _ = user_agent.update_branch(&changes);

            assert_eq!(user_agent.root_key.key, main_user_agent.root_key.key);
        }

        let (root_key, changes) = main_user_agent
            .append_node(main_user_neighbour.lambda)
            .unwrap();

        for user_agent in &mut users_agents {
            _ = user_agent.update_branch(&changes);

            assert_eq!(user_agent.root_key.key, main_user_agent.root_key.key);
        }
    }
}
