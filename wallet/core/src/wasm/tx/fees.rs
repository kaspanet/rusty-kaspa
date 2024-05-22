use crate::imports::*;
use crate::tx::fees::Fees;
use kaspa_wallet_macros::declare_typescript_wasm_interface as declare;
use workflow_wasm::convert::CastFromJs;

///
/// @see {@link IFees}, {@link IGeneratorSettingsObject}, {@link Generator}, {@link estimateTransactions}, {@link createTransactions}
/// @category Wallet SDK
///
#[wasm_bindgen]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, CastFromJs)]
pub enum FeeSource {
    SenderPays,
    ReceiverPays,
}

declare! {
    IFees,
    "IFees | bigint",
    r#"
    /**
     * 
     * @category Wallet SDK
     */
    export interface IFees {
        amount: bigint;
        source?: FeeSource;
    }
    "#,
}

impl TryFrom<IFees> for Fees {
    type Error = Error;
    fn try_from(args: IFees) -> Result<Self> {
        if args.is_undefined() || args.is_null() {
            Ok(Fees::None)
        } else if let Ok(fee) = args.try_as_u64() {
            Ok(Fees::SenderPays(fee))
        } else if let Ok(object) = args.dyn_into::<Object>() {
            let amount = object.get_u64("amount")?;
            if let Some(source) = object.try_get_value("source")? {
                let source = FeeSource::try_cast_from(&source)?;
                match source {
                    FeeSource::SenderPays => Ok(Fees::SenderPays(amount)),
                    FeeSource::ReceiverPays => Ok(Fees::ReceiverPays(amount)),
                }
            } else {
                Ok(Fees::SenderPays(amount))
            }
        } else {
            Err(crate::error::Error::custom("Invalid fee"))
        }
    }
}
