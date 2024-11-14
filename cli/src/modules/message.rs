use kaspa_addresses::Version;
use kaspa_bip32::secp256k1::XOnlyPublicKey;
use kaspa_wallet_core::message::SignMessageOptions;
use kaspa_wallet_core::{
    account::{BIP32_ACCOUNT_KIND, KEYPAIR_ACCOUNT_KIND},
    message::{sign_message, verify_message, PersonalMessage},
};

use crate::imports::*;

#[derive(Default)]
pub struct Message;

#[async_trait]
impl Handler for Message {
    fn verb(&self, _ctx: &Arc<dyn Context>) -> Option<&'static str> {
        Some("message")
    }

    fn help(&self, _ctx: &Arc<dyn Context>) -> &'static str {
        "Sign a message or verify a message signature"
    }

    async fn handle(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, cmd: &str) -> cli::Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        self.main(ctx, argv, cmd).await.map_err(|e| e.into())
    }
}

impl Message {
    async fn main(self: Arc<Self>, ctx: Arc<KaspaCli>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        if argv.is_empty() {
            return self.display_help(ctx, argv).await;
        }

        match argv.first().unwrap().as_str() {
            "sign" => {
                if argv.len() != 2 {
                    return self.display_help(ctx, argv).await;
                }

                let kaspa_address = argv[1].as_str();
                let asked_message = ctx.term().ask(false, "Message: ").await?;
                let message = asked_message.as_str();

                self.sign(ctx, kaspa_address, message).await?;
            }
            "verify" => {
                if argv.len() != 3 {
                    return self.display_help(ctx, argv).await;
                }
                let kaspa_address = argv[1].as_str();
                let signature = argv[2].as_str();
                let asked_message = ctx.term().ask(false, "Message: ").await?;
                let message = asked_message.as_str();

                self.verify(ctx, kaspa_address, signature, message).await?;
            }
            v => {
                tprintln!(ctx, "unknown command: '{v}'\r\n");
                return self.display_help(ctx, argv).await;
            }
        }

        Ok(())
    }

    async fn display_help(self: Arc<Self>, ctx: Arc<KaspaCli>, _argv: Vec<String>) -> Result<()> {
        ctx.term().help(
            &[
                ("sign <kaspa_address>", "Sign a message with the private key that matches the given address. Prompts for message."),
                (
                    "verify <kaspa_address> <signature>",
                    "Verify the signature against the message and kaspa_address. Prompts for message.",
                ),
            ],
            None,
        )?;

        Ok(())
    }

    async fn sign(self: Arc<Self>, ctx: Arc<KaspaCli>, kaspa_address: &str, message: &str) -> Result<()> {
        let kaspa_address = Address::try_from(kaspa_address)?;
        if kaspa_address.version != Version::PubKey {
            return Err(Error::custom("Address not supported for message signing. Only supports PubKey addresses"));
        }

        let pm = PersonalMessage(message);
        let privkey = self.get_address_private_key(&ctx, kaspa_address).await?;
        let sign_options = SignMessageOptions { no_aux_rand: false };

        let sig_result = sign_message(&pm, &privkey, &sign_options);

        match sig_result {
            Ok(signature) => {
                let sig_hex = faster_hex::hex_string(signature.as_slice());
                tprintln!(ctx, "Signature: {}", sig_hex);
                Ok(())
            }
            Err(_) => Err(Error::custom("Message signing failed")),
        }
    }

    async fn verify(self: Arc<Self>, ctx: Arc<KaspaCli>, kaspa_address: &str, signature: &str, message: &str) -> Result<()> {
        let kaspa_address = Address::try_from(kaspa_address)?;
        if kaspa_address.version != Version::PubKey {
            return Err(Error::custom("Address not supported for message signing. Only supports PubKey addresses"));
        }

        let pubkey = XOnlyPublicKey::from_slice(&kaspa_address.payload[0..32]).unwrap();

        let mut signature_hex = [0u8; 64];
        faster_hex::hex_decode(signature.as_bytes(), &mut signature_hex)?;

        let pm = PersonalMessage(message);
        let verify_result = verify_message(&pm, &signature_hex.to_vec(), &pubkey);

        match verify_result {
            Ok(()) => {
                tprintln!(ctx, "Message verified successfully!");
            }
            Err(_) => {
                return Err(Error::custom("Verification failed"));
            }
        }

        Ok(())
    }

    async fn get_address_private_key(self: Arc<Self>, ctx: &Arc<KaspaCli>, kaspa_address: Address) -> Result<[u8; 32]> {
        let account = ctx.wallet().account()?;

        match account.account_kind().as_ref() {
            BIP32_ACCOUNT_KIND => {
                let (wallet_secret, payment_secret) = ctx.ask_wallet_secret(Some(&account)).await?;
                let keydata = account.prv_key_data(wallet_secret).await?;
                let account = account.clone().as_derivation_capable().expect("expecting derivation capable");

                let (receive, change) = account.derivation().addresses_indexes(&[&kaspa_address])?;
                let private_keys = account.create_private_keys(&keydata, &payment_secret, &receive, &change)?;
                for (address, private_key) in private_keys {
                    if kaspa_address == *address {
                        return Ok(private_key.secret_bytes());
                    }
                }

                Err(Error::custom("Could not find address in any derivation path in account"))
            }
            KEYPAIR_ACCOUNT_KIND => {
                let (wallet_secret, payment_secret) = ctx.ask_wallet_secret(Some(&account)).await?;
                let keydata = account.prv_key_data(wallet_secret).await?;
                let decrypted_privkey = keydata.payload.decrypt(payment_secret.as_ref()).unwrap();
                let secretkey = decrypted_privkey.as_secret_key()?.unwrap();
                Ok(secretkey.secret_bytes())
            }
            _ => Err(Error::custom("Unsupported account kind")),
        }
    }
}
