pragma circom 2.1.6;

include "../lib/circuits/babyjubjub/curve.circom";
include "../lib/circuits/utils/switcher.circom";
include "../lib/circuits/bitify/comparators.circom";

// fn compute_next_layer_of_tree(
//         level_nodes: &mut Vec<ARTNode>,
//         level_secrets: &mut Vec<ARTScalarField>,
//         generator: &ART_G,
//     ) -> (Vec<ARTNode>, Vec<ARTScalarField>) {
//         let mut upper_level_nodes = Vec::new();
//         let mut upper_level_secrets = Vec::new();

//         // iterate until level_nodes is empty, then swap it with the next layer
//         while level_nodes.len() > 1 {
//             let left_node = level_nodes.remove(0);
//             let right_node = level_nodes.remove(0);

//             level_secrets.remove(0); // skip the first secret

//             let common_secret = left_node.public_key.mul(level_secrets.remove(0));
//             let secret_hash = Self::iota_function(&common_secret);

//             let node = ARTNode::new(
//                 generator.mul(&secret_hash),
//                 Some(Box::new(left_node)),
//                 Some(Box::new(right_node)),
//             );

//             upper_level_nodes.push(node);
//             upper_level_secrets.push(secret_hash);
//         }

//         if level_nodes.len() == 1 {
//             let first_node = level_nodes.remove(0);
//             upper_level_nodes.push(first_node);
//             let first_secret = level_secrets.remove(0);
//             upper_level_secrets.push(first_secret.clone());
//         }

//         (upper_level_nodes, upper_level_secrets)
//     }


// for both left and right exist
template ARTComputeNext(){
    signal input leftPubkey[2];
    signal input leftSecret;
    signal input rightPubkey[2];
    signal input rightSecret;

    signal input generator[2]; // ?

    signal output outPubkey[2];
    signal output outSecret;

    component mult = BabyjubjubMultiplication();
    mult.scalar <== rightSecret;
    mult.in <== leftPubkey;

    component mult2 = BabyjubjubMultiplication();
    mult2.scalar <== mult.out[0];
    mult2.in <== generator;

    outSecret <== mult.out[0];
    outPubkey <== mult2.out;

    log(outSecret, outPubkey[0], outPubkey[1]);
}


// for right might not exist
template ARTComputeNextZero(){
    signal input leftPubkey[2];
    signal input leftSecret;
    signal input rightPubkey[2];
    signal input rightSecret;

    signal input generator[2]; // ?

    signal output outPubkey[2];
    signal output outSecret;

    component isZero = IsZero();

    isZero.in <== rightSecret;

    component switcher[3];
    for (var i = 0; i < 3; i++){
        switcher[i] = Switcher();
        switcher[i].bool <== isZero.out;
    } 

    component next = ARTComputeNext();
    next.leftPubkey <== leftPubkey;
    next.leftSecret <== leftSecret;
    next.rightPubkey <== rightPubkey;
    // if rightSecret == 0 => isZero == 1, template won`t fail
    // if rightSecret != 0 => isZero == 0 => nothing changed
    next.rightSecret <== rightSecret + isZero.out;
    next.generator <== generator;

    switcher[0].in[0] <== next.outPubkey[0];
    switcher[0].in[1] <== leftPubkey[0];
    outPubkey[0] <== switcher[0].out[0];

    switcher[1].in[0] <== next.outPubkey[1];
    switcher[1].in[1] <== leftPubkey[1];
    outPubkey[1] <== switcher[1].out[0];

    switcher[2].in[0] <== next.outSecret;
    switcher[2].in[1] <== leftSecret;
    outSecret <== switcher[2].out[0];


}