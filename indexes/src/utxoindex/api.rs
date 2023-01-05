#[async_trait]
trait API{
    pub fn is_sync -> bool {}
}

impl API for UtxoIndex {
    pub fn is_sync {

    }

}