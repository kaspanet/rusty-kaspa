use crate::{
    NotifyBlockAddedRequest, NotifyFinalityConflictRequest, NotifyNewBlockTemplateRequest, NotifyPruningPointUtxoSetOverrideRequest,
    NotifySinkBlueScoreChangedRequest, NotifyUtxosChangedRequest, NotifyVirtualChainChangedRequest,
    NotifyVirtualDaaScoreChangedRequest,
};
use kaspa_notify::scope::*;

macro_rules! into_scope {
    ($variant:tt) => {
        paste::paste! {
                impl From<[<Notify $variant Request>]> for Scope {
                fn from (item: [<Notify $variant Request>]) -> Self {
                    [<$variant Scope>]::from(item).into()
                }
            }
        }
    };
}

macro_rules! from {
    // Structure with fields, requiring explicit conversion logic
    ($name:ident : $variant:tt, $body:block) => {
        paste::paste! {
            impl From<[<Notify $variant Request>]> for [<$variant Scope>] {
                fn from($name: [<Notify $variant Request>]) -> Self {
                    $body
                }
            }
            into_scope!($variant);
        }
    };

    // Structure without field
    ($variant:tt) => {
        paste::paste! {
            impl From<[<Notify $variant Request>]> for [<$variant Scope>] {
                fn from(_: [<Notify $variant Request>]) -> Self {
                    Self {}
                }
            }
            into_scope!($variant);
        }
    };
}

from!(BlockAdded);
from!(item: VirtualChainChanged, {
    Self::new(item.include_accepted_transaction_ids)
});
from!(FinalityConflict);
impl From<&NotifyFinalityConflictRequest> for FinalityConflictResolvedScope {
    fn from(_: &NotifyFinalityConflictRequest) -> Self {
        Self::default()
    }
}
from!(item: UtxosChanged, {
    Self::new(item.addresses.clone())
});
from!(SinkBlueScoreChanged);
from!(VirtualDaaScoreChanged);
from!(PruningPointUtxoSetOverride);
from!(NewBlockTemplate);
