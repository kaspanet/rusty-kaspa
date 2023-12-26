use crate::result::Result;
use crate::secret::Secret;
pub struct PrivateKeyDataV1;

pub async fn load_v1_keydata(_phrase: &Secret) -> Result<PrivateKeyDataV1> {
    unimplemented!()
}
