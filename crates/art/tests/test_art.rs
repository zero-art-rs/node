#[cfg(test)]
mod tests {
    use ark_ec::pairing::{Pairing, PairingOutput};
    use ark_ff::{Field, Fp12, PrimeField};
    use ark_std::UniformRand;

    use ark_bn254::{
        Bn254, Config, Fq, Fq12Config, G1Projective as G1, G2Projective as ART_G,
        fr::Fr as ARTScalarField,
    };
    use art::art_trusted_agent::ARTTrustedAgent;
    use art::art_user_agent::ARTUserAgent;
    use art::{self, art::ART, art_node::ARTNode, helper_tools};
    use rand::{Rng, thread_rng};

    fn create_random_secrets(size: usize) -> Vec<Fp12<Fq12Config>> {
        let k = helper_tools::random_non_neutral_scalar_field_element();
        let mut secrets = Vec::new();
        for x in (0..size) {
            let a = G1::rand(&mut thread_rng());
            let b = ART_G::rand(&mut thread_rng());
            let e = Bn254::pairing(a, b).0;

            secrets.push(e);
        }

        secrets
    }

    #[test]
    fn test_art() {
        let secrets = create_random_secrets(100);
        let generator = ART_G::rand(&mut thread_rng());
        let (art, root_key) = ART::new_art_from_secrets(&secrets, &generator);

        println!("art: {:?}", art);
        println!("root_key: {:?}", root_key);
    }

    #[test]
    fn test_art_tree_key_update() {
        let number_of_users = 20;
        let main_user_id = thread_rng().gen_range(0..number_of_users as usize);
        let users = helper_tools::crete_set_of_identities(number_of_users);

        let secrets = create_random_secrets(number_of_users);
        let generator = ART_G::rand(&mut thread_rng());

        let (tree, root_key) = ART::new_art_from_secrets(&secrets, &generator);

        let gamma = ARTScalarField::rand(&mut thread_rng());
        let g2 = ART_G::rand(&mut thread_rng());
        let mut trusted_agent = ARTTrustedAgent::new(gamma, g2);
        let (mut tree, trusted_root_key) = trusted_agent.compute_art_and_ciphertexts(&secrets);

        let tree_json = tree.serialise().unwrap();

        let mut users_agents = Vec::new();
        for i in 0..number_of_users {
            users_agents.push(ARTUserAgent::new(tree.clone(), secrets[i]));
        }

        for user_agent in &users_agents {
            // Assert trusted party and users computed the same tree key. Skip lambda because trusted party cant compute it
            assert_eq!(user_agent.root_key.key, trusted_root_key.key);
        }

        let mut main_user_agent = users_agents.remove(main_user_id);

        // save old lambda to roll back
        let old_lambda = main_user_agent.lambda;
        let (new_key, changes) = main_user_agent.update_key().unwrap();

        for user_agent in &users_agents {
            assert_ne!(trusted_root_key.key, new_key.key);
            assert_ne!(trusted_root_key.lambda, new_key.lambda);
        }

        for user_agent in &mut users_agents {
            _ = user_agent.update_branch(&changes);
            assert_eq!(user_agent.root_key.key, new_key.key);
        }

        let (old_key, changes) = main_user_agent.change_lambda(old_lambda).unwrap();

        assert_eq!(trusted_root_key.key, old_key.key);

        for user_agent in &mut users_agents {
            _ = user_agent.update_branch(&changes);
            assert_eq!(user_agent.root_key.key, old_key.key);
        }
    }

    // #[test]
    // fn test_art_make_temporal_node() {
    //     let number_of_users = 20;
    //     let users = tools::crete_set_of_identities(number_of_users);
    //
    //     let main_user_id = thread_rng().gen_range(0..(number_of_users - 2) as usize);
    //     let mut temporal_user_id = thread_rng().gen_range(0..(number_of_users - 3) as usize);
    //     while temporal_user_id >= main_user_id && temporal_user_id <= main_user_id + 2 {
    //         temporal_user_id = thread_rng().gen_range(0..(number_of_users - 3) as usize);
    //     }
    //
    //     let ibbe = IBBEDel7::setup(number_of_users);
    //
    //     let mut trusted_agent = ARTTrustedAgent::from(&ibbe);
    //     let (mut tree, ciphertexts, trusted_root_key) =
    //         trusted_agent.compute_art_and_ciphertexts(&users);
    //
    //     // let tree_json = tree.serialise().unwrap();
    //
    //     let mut users_agents = Vec::new();
    //     for i in 0..number_of_users {
    //         let sk_id = ibbe.extract(users.get(i as usize).unwrap()).unwrap();
    //         users_agents.push(ARTUserAgent::new(
    //             tree.clone(),
    //             ciphertexts[i as usize],
    //             sk_id,
    //         ));
    //     }
    //
    //     for user_agent in &users_agents {
    //         // Assert trusted party and users computed the same tree key. Skip lambda because trusted party cant compute it
    //         assert_eq!(user_agent.root_key.key, trusted_root_key.key);
    //     }
    //
    //     let mut main_user_agent = users_agents.remove(main_user_id);
    //     let mut temporal_user_agent = users_agents.remove(temporal_user_id);
    //
    //     let (root_key, changes) = main_user_agent
    //         .make_temporal(temporal_user_agent.public_key())
    //         .unwrap();
    //
    //     for user_agent in &mut users_agents {
    //         assert_ne!(user_agent.root_key.key, main_user_agent.root_key.key);
    //         assert_ne!(user_agent.root_key.lambda, main_user_agent.root_key.lambda);
    //
    //         _ = user_agent.update_branch(&changes);
    //
    //         assert_eq!(user_agent.root_key.key, main_user_agent.root_key.key);
    //         assert_ne!(user_agent.root_key.lambda, main_user_agent.root_key.lambda);
    //         assert_eq!(user_agent.tree.size(), (number_of_users - 1) as usize);
    //     }
    //
    //     let mut rng = thread_rng();
    //     let new_lambda = Fp12::<Fq12Config>::rand(&mut rng);
    //
    //     let (root_key, changes) = main_user_agent.append_node(new_lambda).unwrap();
    //
    //     for user_agent in &mut users_agents {
    //         _ = user_agent.update_branch(&changes);
    //
    //         assert_eq!(user_agent.root_key.key, main_user_agent.root_key.key);
    //         assert_ne!(user_agent.root_key.lambda, main_user_agent.root_key.lambda);
    //         assert_eq!(user_agent.tree.size(), number_of_users as usize);
    //     }
    // }
    //
    // #[test]
    // fn test_art_node_removal() {
    //     let number_of_users = 20;
    //     let users = tools::crete_set_of_identities(number_of_users);
    //
    //     let temporal_user_id = thread_rng().gen_range(3..number_of_users as usize);
    //
    //     let ibbe = IBBEDel7::setup(number_of_users);
    //
    //     let mut trusted_agent = ARTTrustedAgent::from(&ibbe);
    //     let (mut tree, ciphertexts, trusted_root_key) =
    //         trusted_agent.compute_art_and_ciphertexts(&users);
    //
    //     // let tree_json = tree.serialise().unwrap();
    //
    //     let mut users_agents = Vec::new();
    //     for i in 0..number_of_users {
    //         let sk_id = ibbe.extract(users.get(i as usize).unwrap()).unwrap();
    //         users_agents.push(ARTUserAgent::new(
    //             tree.clone(),
    //             ciphertexts[i as usize],
    //             sk_id,
    //         ));
    //     }
    //
    //     for user_agent in &users_agents {
    //         // Assert trusted party and users computed the same tree key. Skip lambda because trusted party cant compute it
    //         assert_eq!(user_agent.root_key.key, trusted_root_key.key);
    //     }
    //
    //     let mut main_user_agent = users_agents.remove(0);
    //     let mut main_user_neighbour = users_agents.remove(0);
    //     for i in 0..2 {
    //         let mut for_removal = users_agents.remove(0);
    //
    //         let (root_key, changes) = main_user_agent
    //             .remove_node(for_removal.public_key())
    //             .unwrap();
    //
    //         for user_agent in &mut users_agents {
    //             assert_ne!(user_agent.root_key.key, main_user_agent.root_key.key);
    //
    //             _ = user_agent.update_branch(&changes);
    //
    //             assert_eq!(user_agent.root_key.key, main_user_agent.root_key.key);
    //             assert_eq!(user_agent.tree.size(), (number_of_users - 1 - i) as usize);
    //         }
    //     }
    //
    //     assert!(!main_user_agent.can_remove(users_agents[0].public_key()));
    //
    //     let (root_key, changes) = main_user_agent
    //         .remove_node(main_user_neighbour.public_key())
    //         .unwrap();
    //
    //     for user_agent in &mut users_agents {
    //         assert_ne!(user_agent.root_key.key, main_user_agent.root_key.key);
    //
    //         _ = user_agent.update_branch(&changes);
    //
    //         assert_eq!(user_agent.root_key.key, main_user_agent.root_key.key);
    //     }
    //
    //     let (root_key, changes) = main_user_agent
    //         .append_node(main_user_neighbour.lambda)
    //         .unwrap();
    //
    //     for user_agent in &mut users_agents {
    //         _ = user_agent.update_branch(&changes);
    //
    //         assert_eq!(user_agent.root_key.key, main_user_agent.root_key.key);
    //     }
    // }
    //
    // #[test]
    // fn test_hibbe_encryption() {
    //     let number_of_users = 20u32;
    //     let users = tools::crete_set_of_identities(number_of_users);
    //
    //     let index1 = thread_rng().gen_range(0..number_of_users as usize);
    //     let mut index2 = index1;
    //     while index2 == index1 {
    //         index2 = thread_rng().gen_range(0..number_of_users as usize);
    //     }
    //
    //     let user1 = users.get(index1).unwrap().clone();
    //     let user2 = users.get(index2).unwrap().clone();
    //
    //     let ibbe = IBBEDel7::setup(number_of_users);
    //     let sk_id1 = ibbe.extract(&user1).unwrap();
    //     let sk_id2 = ibbe.extract(&user2).unwrap();
    //
    //     let mut art_agent = ARTTrustedAgent::new(ibbe.msk.clone().unwrap(), ibbe.pk.clone());
    //     let (mut tree, ciphertexts, root_key) = art_agent.compute_art_and_ciphertexts(&users);
    //
    //     // let tree_json = tree.serialise().unwrap();
    //
    //     let mut user1_agent = ARTUserAgent::new(tree.clone(), ciphertexts[index1], sk_id1);
    //
    //     let mut user2_agent = ARTUserAgent::new(tree.clone(), ciphertexts[index2], sk_id2);
    //
    //     let mut hibbe1 = HybridEncryption::new(
    //         ibbe.clone(),
    //         user1_agent,
    //         users.clone(),
    //         user1.clone(),
    //         sk_id1,
    //     );
    //     let mut hibbe2 = HybridEncryption::new(
    //         ibbe.clone(),
    //         user2_agent,
    //         users.clone(),
    //         user2.clone(),
    //         sk_id2,
    //     );
    //
    //     let message = String::from("ffffffffffffffffffffffffff7777777777777777777777777");
    //     let (ciphertext, changes) = hibbe1.encrypt(message.clone());
    //     let decrypted_message = hibbe2.decrypt(ciphertext.clone(), &changes.clone());
    //     // Assert the second user can decrypt the message
    //     assert_eq!(message, decrypted_message);
    //
    //     let message2 = String::from("ccccccccccccccccccccccccccc7777777777777777777777777");
    //     let (ciphertext2, changes2) = hibbe2.encrypt(message2.clone());
    //     let decrypted_message2 = hibbe1.decrypt(ciphertext2.clone(), &changes2.clone());
    //     // Assert users can have a conversation
    //     assert_eq!(message2, decrypted_message2);
    // }
}
