//!
//! v0 (KDX-style) account data decryption
//!

use crate::error::Error;
use crate::imports::*;
use cfb_mode::cipher::AsyncStreamCipher;
use cfb_mode::cipher::KeyIvInit;
use evpkdf::evpkdf;
use faster_hex::{hex_decode, hex_string};
use kaspa_bip32::{ExtendedPrivateKey, Language, Mnemonic, Prefix, SecretKey};
use md5::Md5;
use pbkdf2::{hmac::Hmac, pbkdf2};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use std::path::PathBuf;
#[allow(unused_imports)]
use workflow_core::env;
use workflow_core::runtime;
use workflow_store::fs;
use workflow_store::fs::exists_with_options;
use workflow_store::fs::read_json_with_options;
use workflow_store::fs::Options;
use zeroize::Zeroize;

type Aes256CfbDec = cfb_mode::Decryptor<aes::Aes256>;

#[derive(Serialize, Deserialize, Debug)]
#[allow(non_snake_case)]
pub struct PrivateKeyDataImplV0 {
    privKey: String,
    seedPhrase: String,
}

impl Drop for PrivateKeyDataImplV0 {
    fn drop(&mut self) {
        self.privKey.zeroize();
        self.seedPhrase.zeroize();
    }
}

pub fn create_master_key_from_mnemonics(seed_words: &str) -> Result<ExtendedPrivateKey<SecretKey>> {
    let mnemonic = Mnemonic::new(seed_words, Language::English)?;
    let seed = mnemonic.to_seed("");
    let xprv = ExtendedPrivateKey::<SecretKey>::new(seed).unwrap();
    Ok(xprv)
}

#[derive(Debug)]
pub struct PrivateKeyDataV0 {
    pub mnemonic: String,
}

impl PrivateKeyDataV0 {
    pub fn key(&self) -> Result<ExtendedPrivateKey<SecretKey>> {
        create_master_key_from_mnemonics(&self.mnemonic)
    }
    pub fn xprv(&self) -> Result<String> {
        Ok(self.key()?.to_string(Prefix::XPRV).to_string())
    }
}

impl Drop for PrivateKeyDataV0 {
    fn drop(&mut self) {
        self.mnemonic.zeroize();
    }
}

impl TryFrom<PrivateKeyDataImplV0> for PrivateKeyDataV0 {
    type Error = Error;
    fn try_from(value: PrivateKeyDataImplV0) -> Result<Self, Self::Error> {
        let key = create_master_key_from_mnemonics(&value.seedPhrase)?.to_string(Prefix::XPRV).to_string();
        if !key.eq(&value.privKey) {
            return Err(Error::Custom("privKey dose not matches with drived xprv from given seeds".into()));
        }

        Ok(PrivateKeyDataV0 { mnemonic: value.seedPhrase.clone() })
    }
}

#[derive(Clone, Debug)]
pub struct CipherData {
    content: Vec<u8>,
    iv: Vec<u8>,
    salt: Vec<u8>,
    pass_salt: Vec<u8>,
}

fn get_v0_parts(data: &str) -> Result<CipherData> {
    let mut ptr = data;
    let mut list = vec![];
    while !ptr.is_empty() {
        let len: usize = ptr[0..5].parse().unwrap();
        let mut data = vec![0; len / 2];
        hex_decode(ptr[5..(5 + len)].as_bytes(), &mut data).unwrap();
        list.push(data);
        ptr = &ptr[(5 + len)..];
    }

    let cipher_data = CipherData { content: list[0].clone(), iv: list[1].clone(), salt: list[2].clone(), pass_salt: list[3].clone() };

    Ok(cipher_data)
}

pub fn get_v0_keydata(data: &str, phrase: &Secret) -> Result<PrivateKeyDataV0> {
    let mut json = get_v0_string(data, phrase)?;
    let keydata: PrivateKeyDataImplV0 = serde_json::from_str(&json).unwrap();
    json.zeroize();
    keydata.try_into()
}
fn get_v0_string(data: &str, phrase: &Secret) -> Result<String> {
    let CipherData { content, iv, salt, pass_salt } = get_v0_parts(data)?;

    let mut key = [0u8; 64];
    pbkdf2::<Hmac<Sha1>>(phrase.as_ref(), &pass_salt, 1000, &mut key).expect("pbkdf2 failure");

    let key_hex = hex_string(&key).as_bytes().to_vec();
    key.zeroize();
    let mut key_out = [0u8; 32];
    evpkdf::<Md5>(&key_hex, &salt, 1, &mut key_out);
    let key: [u8; 32] = key_out[0..32].try_into().unwrap();
    let iv: [u8; 16] = iv[0..16].try_into().unwrap();

    let mut content = content.to_vec();
    let json = aes_decrypt_v0(&key, &iv, &mut content)?;
    content.zeroize();
    Ok(json)
}

fn aes_decrypt_v0(key: &[u8], iv: &[u8], content: &mut [u8]) -> Result<String> {
    Aes256CfbDec::new(key.into(), iv.into()).decrypt(content);
    let last = content.len() - *content.last().unwrap() as usize;
    String::from_utf8(content[0..last].to_vec()).map_err(|_| Error::custom("Unable to decrypt wallet - invalid password"))
}

// ---
// {"type":"kaspa-wallet","encryption":"default","version":1,"generator":"pwa","wallet":{"mnemonic":"hex"}}

#[derive(Deserialize)]
struct Wallet {
    mnemonic: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct Envelope {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub encryption: Option<String>,
    pub default: Option<String>,
    pub generator: Option<String>,
    pub wallet: Wallet,
}

fn legacy_v0_keydata_location() -> Result<(PathBuf, Options)> {
    let filename = if runtime::is_windows() {
        let appdata = env::var("APPDATA")?;
        fs::resolve_path(&format!("{appdata}/Kaspa/kaspa.kpk"))?
    } else if runtime::is_macos() {
        fs::resolve_path("~/Library/Application Support/Kaspa/kaspa.kpk")?
    } else {
        fs::resolve_path("~/.kaspa/kaspa.kpk")?
    };

    let options = workflow_store::fs::Options::with_local_storage_key("kaspa-wallet");

    Ok((filename, options))
}

pub async fn exists_legacy_v0_keydata() -> Result<bool> {
    let (filename, options) = legacy_v0_keydata_location()?;
    Ok(exists_with_options(&filename, options).await?)
}

pub async fn load_v0_keydata(phrase: &Secret) -> Result<PrivateKeyDataV0> {
    let (filename, options) = legacy_v0_keydata_location()?;

    let wallet = if runtime::is_web() {
        read_json_with_options::<Wallet>(&filename, options).await?
    } else {
        read_json_with_options::<Envelope>(&filename, options).await?.wallet
    };

    get_v0_keydata(&wallet.mnemonic, phrase)
}

// ---

#[test]
fn test_v0_keydata() {
    let data = "004488a555e6fd87f5c96bc1a735d65ff3e8b9005218ca4bff9f5460c0b75e7c4396c530a3004896b5846de2741cf32b1a367aa581ba8e035f9229e0e24fb383bfc3ea731c842541fbf4f6c0c4e6f7306d08ea28b40dc74d0c60e75c82d94d401d5716a749c42cf0d20c78a43d7e4b97ce2a1dd146e38b080c9fcaff9ec5bdcae09704a9f1cefefe5375f768cc92135adf89018a1ee9b595baaa72729306380e7c221d9000c77d25830e1af566255f0a6213285861b06305ef1f04b4c1de1b897644b26121f81eddb84b54efe88c46bd0b2e38057797ee5552e745759967a856970a900032cb852cabdfe2c6f73a9d84c462067bdb0001631faffd91cb36a3900032f6f66cc22b119ced5b340ab5af961dcd";
    let secret = Secret::new(b"Hunter44!".to_vec());
    let keydata = get_v0_keydata(data, &secret).unwrap();
    assert_eq!(
        keydata.xprv().unwrap(),
        "xprv9s21ZrQH143K4QGJm8SHAMTuw8ca8Hk4DdG31eNcMARmkcg4tojEmeYx6dtqXAJbodJJ6FvJLKtBygB7hYiDXNhn21CQ1j5aJmfahJfwN8f"
    );
    assert_eq!(keydata.mnemonic, "interest denial place quick stay suit token shadow side ski knife entire");
}

#[test]
fn test_v0_padding() {
    let secret = Secret::new(b"Hunter44!".to_vec());
    let data = [
        "00032b19a1b4777dc4575823cc48813cc571f0003226c7724cbc174a2cb29880fb70f432a70001697b6b06bea28cf01000327c6630d8af36cd60072cf052f92c18f1",
        "00032dca0c4fab9a1c9bc2b77317c642158de000322fd05504fdeb55bd4f90cd2ffe5a2f45000164c4930b3123fbd5f000327c08edce7cfb4e4ae7e0eaf4c7f0cae3",
        "000321ac2bd108d773a3416fe555d9b85fb9600032829694b4c9abdeb1b535ce73b068e888000169549ba818472cc54000324a0bfdf93c9bd4fe813e82a92bdb09d0",
        "00032e2c7256dd89dee114e43747e170529c70003284c6a28aa7a7e71b8578f43e0ef637de00016666b186817c54b1100032b3218237c20564377f405e5b606364ba",
        "0003226b652c6450125312a5cc944799d5de300032e3ab1784746c9c958e666b4de0483b6c00016ab5c3e37cac2a2d800032f9f163b97bf55450c8e43af204805212",
        "0003211a26bfc38e49a1a0743646a87c5d2bb00032e5092080cd67dfcfbaf730b9ab93da6f00016df32352584da5f2d00032cb8d94bdd109e851959db76d01a4991c",
        "0003235919cd3d3d19d1ea4fa344753b596b900032fa0198a4e88588d75725a44a05b0e046000169d8909db512e542b00032a813d4d9eca45d6b4e9041e97f9edf24",
        "000328569f28479d172ec5239a52cd6848d21000327502a6623d4118880613f0cffefd051200016f7c23efff97fa04000032e5915413dc5488b6fc4590c20dd03f53",
        "000329aeca06b5bd8fa3cab61194a8d7e401f000323cf6903aa8409aafec66abdc1c234237000162396e43f66cc4df800032b5dc64fb362f93c8dd83dbcff3be68e1",
        "00032ad83e96153999b24b9c7f16d2fba3dc30003254019ee603dd10a9735f96bb0a74c1c400016f25f2e2127d5b59400032b2bf1b7148defb61b78aca0c2c9987ee",
        "0003273223b136bf6e26889bbdf5606f30d8a000323554da992200d0d8ac140304b828c5a100016a2b81b2e4bef10bb0003255e1b98c12d8104f26b7b542d0c161e4",
        "00032716eef365e9188b19c502e87c81245c800032fd9d490de0e1314d51a9294f2d7cbbc7000166fe89cbd6ee3993500032ddd39a0c6f22852f8c5c2abf134e2213",
        "00032dbf83998110f214897b4239decac747600032cdfb5427e0e3d55744784945ad8d71ee00016599fbb71532ba230000327d89c221d8c4d711dfc8b21cc6e32b16",
        "00032325afb8538f6f98b038dd81187442de800032ee5196ccc9970d109748c68492ab9177000168bb2a7c4fd1cd15600032d5a4462aee656391215e7562a70c4484",
        "0003226fbc925d6d6424842bd65b340917a4200032b8dc13cdade632f139f5ca012f8328c6000162d57760279591fb00003200ab652b4dfa5952ee0c107273571da6",
        "000648a07e19d22e5a4d67c39dee71cf42bcd70a641ede2c047ca0c8f5151823cc9d900032372eb8909c7080fca8462b0014ecebea000163fc07de7a1b3222900032edd6d5ec2fb52f05f360e64a187a9a81",
        "00064c1e52faaca14de11b537eaa82c744882326b7168751759a997ccaf7ad74638360003214f093beb984a590ea6ceb0de31acfac0001619d110e102ba3a8700032cad5720da044c671dea35531f0c13d96",
        "000645897f22514997421169701fc035286f59ddf8df21cad46d366bcc94855897ff20003235d6bbb69b437ac62d204fadf6ce6b8700016c083151aabbb4ad20003264fc31cf3a087ce22e00ca6b4afa7e87",
        "0006472b9df60a6c80ae8f764336c6f17ccc58fe11d26f08374653208c6d025707ab7000329a3a1831279c37b21fa27095918ebe2c000162524bcb44f546ae000032c41bddd17cccf19cdaca6a049d9bb513",
    ];

    let s = "01234567890abcdefgh";
    for n in 0..s.len() {
        let s = &s[0..n + 1];
        let text = get_v0_string(data[n], &secret).unwrap();
        // println!("{}: s: {} text: {}", n, s, text);
        assert_eq!(text, s);
    }
}
