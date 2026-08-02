pub struct KeyEventAction<'a> {
    action: Box<dyn Fn() + Send + Sync>,
    condition: Vec<Source<'a>>,
}
impl<'a> KeyEventAction<'a> {
    pub fn new(action: Box<dyn Fn() + Send + Sync>, condition: Vec<Source<'a>>) -> Self {
        Self { action, condition }
    }
    pub fn execute(&self) -> bool {
        let conditions_met = self.condition.iter().all(|check| check.eval());
        if conditions_met {
            (self.action)();
        }
        conditions_met
    }
}
