pub mod arc;
pub mod binary_heap;
pub mod channel;
pub mod option;
pub mod refs;
pub mod macros;
pub mod triggers;
pub mod counter;

#[cfg(test)]
pub mod test {
    use crate::counter::*;

    #[test]
    fn test_counter() {
        let intial: ahash::AHashSet<char> = vec!['A','B'].into_iter().collect();
        let a: ahash::AHashSet<char> = vec!['A'].into_iter().collect();
        let mut counter = AHashCounter::<char>::new();
        
        (0..2).for_each(|_| {counter.add(intial.clone()); ()}); //we increment intial counts to 2.
        assert_eq!(counter.get_active_set(), intial.clone()); 
        
        assert!(counter.remove(a.clone()).is_empty()); //we decrement `A` twice, secound one should return none-empty
        assert_eq!(counter.remove(a.clone()).len(), 1);
        assert_eq!(counter.get_active_set(), ahash::AHashSet::from_iter(vec!['B']));
        
        assert_eq!(counter.add(a.clone()).len(), 1);  //we incremant `A` twice, first one should return none-empty 
        assert!(counter.add(a.clone()).is_empty());
        assert_eq!(counter.get_active_set(), intial);
    }
}