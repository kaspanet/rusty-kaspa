use kaspa_hashes::{Hash, HasherBase, MerkleBranchHash, ZERO_HASH};
#[derive(Default)]
pub enum LeafRoute
 {
    #[default]
    Left,
    Right,
}
pub fn calc_merkle_root_legacy(hashes: impl ExactSizeIterator<Item = Hash>) -> Hash {
    if hashes.len() == 0 {
        return ZERO_HASH;
    }
    let next_pot = hashes.len().next_power_of_two();
    let vec_len = 2 * next_pot - 1;
    let mut merkles = vec![None; vec_len];
    for (i, hash) in hashes.enumerate() {
        merkles[i] = Some(hash);
    }
    let mut offset = next_pot;
    for i in (0..vec_len - 1).step_by(2) {
        if merkles[i].is_none() {
            merkles[offset] = None;
        } else {
            merkles[offset] = Some(merkle_hash(merkles[i].unwrap(), merkles[i + 1].unwrap_or(ZERO_HASH)));
        }
        offset += 1
    }
    merkles.last().unwrap().unwrap()
}
fn derive_merkle_tree(hashes: impl ExactSizeIterator<Item = Hash>)->Vec<Option<Hash>>{
    if hashes.len() == 0 {
        return vec!(Some(ZERO_HASH));
    }
    let next_pot = hashes.len().next_power_of_two();//maximal number of  leaves in last level of tree
    let vec_len = 2 * next_pot - 1;//maximal number of nodes in tree
    let mut merkles = vec![None; vec_len];
    //store leaves in the bottom level of the tree 
    for (i, hash) in hashes.enumerate() {
        merkles[i] = Some(hash);
    }
    //compute merkle tree
    let mut offset = next_pot;
    for i in (0..vec_len - 1).step_by(2) {
            
        if merkles[i].is_none() {
            merkles[offset] = None;
        } else {
            merkles[offset] = Some(merkle_hash(merkles[i].unwrap(), merkles[i + 1].unwrap_or(ZERO_HASH)));
        }
        offset += 1
    }
    merkles
}


pub fn calc_merkle_root(hashes: impl ExactSizeIterator<Item = Hash>) -> Hash {

    // create the merkle witness for an arbitrary transaction index,
    // the last element in the witness is always the merkle tree root.
    let merkles=derive_merkle_tree(hashes);
    merkles.last().unwrap().unwrap()
}
pub fn create_merkle_witness(hashes: impl ExactSizeIterator<Item = Hash>,leaf_index:usize) -> Vec<(Hash,LeafRoute
)>{
    let next_pot = hashes.len().next_power_of_two();//maximal number of  leaves in last level of tree
    let vec_len = 2 * next_pot - 1;//maximal number of nodes in mekle tree
    let merkles=derive_merkle_tree(hashes);
    let mut witness_vec=vec![];

    let mut level_start=0;
    let mut level_length=next_pot;
    let mut level_index=leaf_index;
    //iterate over the indices per level corresponding to the route from leaf to the root and collect their "matches"
    //alongside the path - the merkle root itself is not collected
    while level_length>1{
        witness_vec.push({
            //the leaf_index describes the indexing of the leaf itself per level, we store its "companion" hash as witness 
            if level_index%2==0  {(merkles[level_start+level_index+1].unwrap_or(ZERO_HASH),LeafRoute::Left)}//edge case relevant to the leaf level only
            else                 {(merkles[level_start+level_index-1].unwrap(),LeafRoute::Right)}
        });

        level_start=level_start+level_length;
        level_length=level_length/2;
        level_index=level_index/2;
    }
    assert_eq!(level_start,vec_len-1);
    assert_eq!(level_index,0);
    witness_vec
}


pub fn verify_merkle_witness(witness_vec: &Vec<(Hash,LeafRoute)>,leaf_value:Hash,merkle_root_hash:Hash) ->bool{
    let vec_len = witness_vec.len();
    // if(vec_len==0)
    //     panic!("witness vector is not of a valid size")

    let mut current_hash=leaf_value;
    for i in 0..vec_len {//ignore root
        //the LeafRoute describes which branch the leaf is at
        match witness_vec[i].1 {
            LeafRoute
            ::Right
             =>  {current_hash= merkle_hash(witness_vec[i].0,current_hash);}
            LeafRoute
            ::Left
            =>  {current_hash= merkle_hash(current_hash,witness_vec[i].0);}
    }
       
    }
    return current_hash==merkle_root_hash;
 
}

fn merkle_hash(left_node: Hash, right_node: Hash) -> Hash {
    let mut hasher = MerkleBranchHash::new();
    hasher.update(left_node).update(right_node);
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use kaspa_hashes::Hash;
    use kaspa_hashes::ZERO_HASH;
    use super::{create_merkle_witness,calc_merkle_root_legacy,calc_merkle_root,verify_merkle_witness};
    #[test]
    fn test_witnesses_basic(){
        // test the case of the empty tree which gets missed in the more general test, alongside a single leaf and double leaf tree
        let empty_vec=vec!();
        let hash_str_1 = "8e40af02265360d59f4ecf9ae9ebf8f7";
        let hash_str_2 = "88700k0226a660d7a14ecfpae9eb0no2";
        let mut result = [0u8; 32];

        let mut bytes:Vec<u8> = hash_str_1.as_bytes().to_vec();
        result[..bytes.len()].copy_from_slice(&bytes);
        let hash1 = Hash::from_bytes(result);

        bytes = hash_str_2.as_bytes().to_vec();
        result[..bytes.len()].copy_from_slice(&bytes);
        let hash2 = Hash::from_bytes(result);

        let single_vec=vec!(hash1);
        let double_vec=vec!(hash1,hash2);
        //Zero hash sent as leaf value when there is no leaf
        assert!(verify_merkle_witness(&create_merkle_witness(empty_vec.clone().into_iter(),0),ZERO_HASH,calc_merkle_root(empty_vec.clone().into_iter())));
    
        assert!(verify_merkle_witness(&create_merkle_witness(single_vec.clone().into_iter(),0),hash1,calc_merkle_root(single_vec.clone().into_iter())));
        assert!(verify_merkle_witness(&create_merkle_witness(double_vec.clone().into_iter(),0),hash1,calc_merkle_root(double_vec.clone().into_iter())));
        assert!(verify_merkle_witness(&create_merkle_witness(double_vec.clone().into_iter(),1),hash2,calc_merkle_root(double_vec.clone().into_iter())));


    }
    #[test]
    fn test_witnesses_consistency(){
        const TREE_LENGTH:usize=30;
        let mut hash_vec=vec!();
        let hash_str_1 = "8e40af02265360d59f4ecf9ae9ebf8f7";
        let bytes:Vec<u8> = hash_str_1.as_bytes().to_vec();

        for i in 0..TREE_LENGTH{
            let mut new_bytes = bytes.clone();
            new_bytes[i] = b'5';
            let mut result = [0u8; 32];
            result[..new_bytes.len()].copy_from_slice(&new_bytes);
            let hash = Hash::from_bytes(result);

            hash_vec.push(hash);
        }
        for j in 0..TREE_LENGTH{
        // let j=TREE_LENGTH-1;
            for i in 0..j{
                let witness=create_merkle_witness(hash_vec.clone().into_iter().take(j),i);
                let merkle1=calc_merkle_root(hash_vec.clone().into_iter().take(j));
                let merkle2=calc_merkle_root_legacy(hash_vec.clone().into_iter().take(j));
                assert_eq!(merkle1,merkle2);
                assert!(verify_merkle_witness(&witness,hash_vec[i],merkle1));
                println!("{}",merkle1);
            }
        } 



        
    }
}