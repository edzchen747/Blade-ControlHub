pub enum Source<'a> {
    IsTrue(&'a AtomicBool),
    IsFalse(&'a AtomicBool),
    IsXOR(&'a AtomicBool, &'a AtomicBool),
}

impl<'a> Source<'a> {
    fn eval(&self) -> bool {
        match self {
            Self::IsTrue(atomic) => atomic.load(Ordering::SeqCst),
            Self::IsFalse(atomic) => !atomic.load(Ordering::SeqCst),
            Self::IsXOR(atomic1, atomic2) => {
                atomic1.load(Ordering::SeqCst) ^ atomic2.load(Ordering::SeqCst)
            }
        }
    }
}

