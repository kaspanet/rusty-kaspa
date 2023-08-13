# Subscriptions processing in the notification system
## UtxosChanged Subscription
### Mutation
- **All** -->	*mutation.active() = true && addresses empty*
- **Add(A)** -->	*mutation.active() = true && addresses = A*
- **Remove(R)** -->	*mutation.active() = false && addresses = R*
- **None** -->	*mutation.active() = false && addresses empty*

*Where A and R are address sets and addresses is the payload of the mutation scope*

### State
- **None** -->	*subscription.active = false && subscription.addresses empty*
- **Selected(S)** -->	*subscription.active = true && subscription.addresses = S*
- **All** -->	*subscription.active = true && subscription.addresses empty*

*Where S is an address set*

### Mutation of a Single Subscription
As one operation is applied to a Single Subscription, 12 cases arise.

For every combination of Mutation and State, a list of atomic mutations and a resulting state of the Subscription is indicated in the following table.

|**State**|**Mutation**<br>All|**Mutation**<br>Add(A)|**Mutation**<br>Remove(R)|**Mutation**<br>None|
| :-: | :-: | :-: | :-: | :-: |
|**None**|<p>—</p><p>+ All</p><p>----------------</p><p>All</p>|<p>—</p><p>+ A</p><p>----------------</p><p>Selected(A)</p>|<p>—</p><p>—</p><p>----------------</p><p>None</p>|<p>—</p><p>—</p><p>----------------</p><p>None</p>|
|**Selected(S)**|<p>- S</p><p>+ All</p><p>----------------</p><p>All</p>|<p>—</p><p>+ (A – S)</p><p>----------------</p><p>Selected(A ∪ S)</p>|<p>—</p><p>- (R ∩ S)</p><p>----------------</p><p>Selected(S – R)</p>|<p>—</p><p>- S</p><p>----------------</p><p>None</p>|
|**All**|<p>—</p><p>—</p><p>----------------</p><p>All</p>|<p>+ A</p><p>- All</p><p>----------------</p><p>Selected(A)</p>|<p>—</p><p>—</p><p>----------------</p><p>All</p>|<p>—</p><p>- All</p><p>----------------</p><p>None</p>|

The atomic mutations are applied to the subscription, which leads to a new subscription state.

The same mutations must be propagated to a Compounded Subscription handling UtxosChanged notifications. For every mutation submitted (see next section) by calling fn `compound` a resulting `Option<SubscribeMessage<NotificationType>>>` is returned.

The value returned by the last call, if `Some`, must be propagated to the parent.

### Compounded Subscription UtxosChanged
This structure contains counters for `All` and for every address registered in a set.

The possible mutations are:

|**Mutation**|**Process pseudo-code**|**Return pseudo-code**|
| :- | :- | :- |
|**Add(All)**|<pre>Increment the `All` counter</pre>|<pre>If All == 1<br/>   Some(SubscribeMessage::StartEvent(NotificationType::UtxosChanged(empty)))<br>Else<br/>   None</pre>|
|**Add(A)**|<pre>For each `a` in `A`<br/>   inc counter of `a`<br/>   If counter == 1 add `a` to `B`</pre>|<pre>If B is not empty and All == 0<br/>   Some(SubscribeMessage::StartEvent(NotificationType::UtxosChanged(B)))<br/>Else<br/>   None</pre>|
|**Remove(R)**|<pre>For each `r` in `R`<br/>   dec counter of r<br/>   If counter == 0 add `r` to `S`</pre>|<pre>If `S` is not empty and `All` == 0<br/>   Some(SubscribeMessage::StopEvent(NotificationType::UtxosChanged(S)))</br>Else<br/>   None</pre>|
|**Remove(All)**|<pre>Decrement the `All` counter</pre>|<pre>If `All` == 0<br/>   Build `S` with every `a` in addresses having counter > 0</br>   If `S` is not empty<br/>      Some(SubscribeMessage::StartEvent(NotificationType::UtxosChanged(S)))</br>   Else<br/>      Some(SubscribeMessage::StopEvent(NotificationType::UtxosChanged(empty)))</br>Else<br/>   None</pre>|

It is advised to clean the address set of the Compounded Subscription, removing the addresses which counter reaches 0.

## VirtualSelectedParentChainChanged
### Mutation
- **All** -->	*mutation: active() = true && include\_accepted\_transaction\_ids = true*
- **Reduced** -->	*mutation: active() = true && include\_accepted\_transaction\_ids = false*
- **None (via reduced)** -->	*mutation.active() = false && include\_accepted\_transaction\_ids = false*
- **None (via all)** -->	*mutation.active() = false && include\_accepted\_transaction\_ids = true*
### State
- **None** -->	*subscription: active = false*
- **Reduced** -->	*subscription: active = true && include\_accepted\_transaction\_ids = false*
- **All** -->	*subscription: active = true && include\_accepted\_transaction\_ids = true*
### Mutation of a Single Subscription
As one operation is applied to a Single Subscription, 9 cases arise.

For every combination of Mutation and State, a list of atomic mutations and a resulting state of the Subscription is indicated in the following table.

|**State**|<p>**Mutation**</p><p>All</p>|<p>**Mutation**</p><p>Reduced</p>|<p>**Mutation**</p><p>None</p>|
| :-: | :-: | :-: | :-: |
|None|<p>—</p><p>+ All</p><p>----------------</p><p>All</p>|<p>—</p><p>+ Reduced</p><p>----------------</p><p>Reduced</p>|<p>—</p><p>—</p><p>----------------</p><p>None</p>|
|Reduced|<p>- Reduced</p><p>+ All</p><p>----------------</p><p>All</p>|<p>—</p><p>—</p><p>----------------</p><p>Reduced</p>|<p>—</p><p>- Reduced</p><p>----------------</p><p>None</p>|
|All|<p>—</p><p>—</p><p>----------------</p><p>All</p>|<p>+ Reduced</p><p>- All</p><p>----------------</p><p>Reduced</p>|<p>—</p><p>- All</p><p>----------------</p><p>None</p>|

### Compounded Subscription VirtualSelectedParentChainChanged
This structure contains counters for `All` and `Reduced`.

The possible mutations are:

|**Mutation**|**Process pseudo-code**|**Return pseudo-code**|
| :- | :- | :- |
|**Add `All`**|<pre>Increment `All`</pre>|<pre>If `All` == 1<br/>   Some(SubscribeMessage::StartEvent(NotificationType::VirtualSelectedParentChainChanged(true)))<br/>Else</br>   None</pre>|
|**Add `Reduced`**|<pre>Increment `Reduced`</pre>|<pre>If `Reduced` == 1 and `All` == 0<br/>   Some(SubscribeMessage::StartEvent(NotificationType::VirtualSelectedParentChainChanged(false)))<br/>Else</br>   None</pre>|
|**Remove `Reduced`**|<pre>Decrement `Reduced`</pre>|<pre>If `Reduced` == 0 and `All` == 0<br/>   Some(SubscribeMessage::StopEvent(NotificationType::VirtualSelectedParentChainChanged(false)))<br/>Else</br>   None</pre>|
|**Remove `All`**|<pre>Decrement `All`</pre>|<pre>If `All` == 0<br/>   If `Reduced` > 0</br>      Some(SubscribeMessage::StartEvent(NotificationType::VirtualSelectedParentChainChanged(false)))<br/>   Else<br/>      Some(SubscribeMessage::StopEvent(NotificationType::VirtualSelectedParentChainChanged(true)))</br>Else<br/>   None</pre>|
