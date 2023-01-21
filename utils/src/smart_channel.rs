struct SmartReciever<T, K> {
    receiver: Receiver<T>,
    registry: Hashset<k, AtomicBool>,
}

struct SmartSender<T, K> {
    sender: Sender<T>,
    is_active: AtomicBool,
    mapping: HashSet<T, AtomicBool>,
}

impl SmartReciever<T, K> {
    fn send(&self, msg: T) {
        if self.mapping.get(T).load() {
            self.send(),
        }
    }
}


struct SmartChannel<T> {
    receiver: Receiver<T>,
    sender: Sender<T>,
}